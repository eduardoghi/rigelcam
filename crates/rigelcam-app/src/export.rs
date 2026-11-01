//! Reads a stretch of a recording from the recorder and saves it as an MP4, with the video as the
//! camera encoded it.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;
use rigelcam_core::clips;
use rigelcam_core::dahua::{Client, Error as RecorderError};
use rigelcam_core::dav::{Codec, Frame, Reader};

use crate::camera_stream::missing_from;

const ELEMENTS: [&str; 5] = ["appsrc", "h264parse", "h265parse", "mp4mux", "filesink"];
/// How much video can wait for the disk before reading from the recorder pauses.
const MAX_QUEUED_BYTES: u64 = 32 * 1024 * 1024;
const QUEUE_CHECK_INTERVAL: Duration = Duration::from_millis(20);
/// Added to each side, since a video can only start at a key frame and the recorder stops up to 2
/// seconds short of the end. A longer video is better than one missing what was asked for.
const MARGIN_SECONDS: u32 = 2;
pub const LAST_SECOND_OF_DAY: u32 = 86_399;
/// Longest stretch asked for at once. Above something between 2 and 3 hours recorders answer as if
/// there were no recordings, and each request is one login in their log.
const WINDOW_SECONDS: u32 = 3_600;
/// A response stops up to 2 seconds short of its end and starts at no particular frame, so the next
/// one starts before that. The reader drops what repeats.
const WINDOW_OVERLAP_SECONDS: u32 = 15;
const CHECK_INTERVAL: Duration = Duration::from_millis(200);
/// A recorder that stops sending leaves the connection open, so reading would never return.
const STALL_LIMIT: Duration = Duration::from_secs(30);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);
const READ_SIZE: usize = 64 * 1024;
/// Pieces of the download waiting for the disk, which is what keeps a slow disk out of memory.
const PIECES_IN_FLIGHT: usize = 8;

pub struct Job {
    pub recorder: i32,
    pub channel: u32,
    pub name: String,
    pub date: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cancelled")]
    Cancelled,
    #[error("no recordings in this stretch")]
    Empty,
    #[error("the recorder stopped sending the video")]
    Stopped,
    #[error("GStreamer element {0} is missing")]
    Missing(String),
    #[error("{0}")]
    Write(String),
    #[error(transparent)]
    Recorder(#[from] RecorderError),
}

/// Reads a stretch from the recorder on its own thread, in as many requests as it takes, since a
/// read can't be interrupted and the recorder can stop answering. The channel closes at the end.
/// Recorders answer a stretch with no recordings as a bad request, and `through_gaps` goes on to
/// the next window after one.
pub fn download(
    client: Arc<Client>,
    channel: u32,
    date: String,
    start: u32,
    end: u32,
    through_gaps: bool,
) -> Receiver<Result<Vec<u8>, Error>> {
    let (sender, pieces) = sync_channel(PIECES_IN_FLIGHT);
    std::thread::spawn(move || {
        let mut from = start;
        loop {
            let to = end.min(from + WINDOW_SECONDS);
            match client.download(channel, &date, from, to) {
                Ok(mut body) => loop {
                    let mut piece = vec![0; READ_SIZE];
                    match body.read(&mut piece) {
                        Ok(0) => break,
                        Ok(size) => {
                            piece.truncate(size);
                            // Fails when nobody is reading anymore, which also closes the request.
                            if sender.send(Ok(piece)).is_err() {
                                return;
                            }
                        }
                        Err(_) => {
                            let _ = sender.send(Err(Error::Stopped));
                            return;
                        }
                    }
                },
                Err(RecorderError::Status(400)) if through_gaps => {}
                Err(RecorderError::Status(400)) => return,
                Err(error) => {
                    let _ = sender.send(Err(error.into()));
                    return;
                }
            }
            if to >= end {
                return;
            }
            from = to - WINDOW_OVERLAP_SECONDS;
        }
    });
    pieces
}

/// A frame as GStreamer takes it, at `milliseconds` of the stream.
pub fn buffer_of(frame: Frame, milliseconds: u64) -> gst::Buffer {
    let time = gst::ClockTime::from_mseconds(milliseconds);
    let mut buffer = gst::Buffer::from_mut_slice(frame.data);
    {
        let buffer = buffer.get_mut().expect("just created");
        buffer.set_pts(time);
        buffer.set_dts(time);
        if !frame.key {
            buffer.set_flags(gst::BufferFlags::DELTA_UNIT);
        }
    }
    buffer
}

/// Writes the MP4 while the frames arrive, without decoding them.
struct Muxer {
    pipeline: gst::Pipeline,
    source: AppSrc,
}

impl Muxer {
    fn new(codec: Codec, path: &Path) -> Result<Self, Error> {
        let (parser, caps) = match codec {
            Codec::H264 => ("h264parse", "video/x-h264"),
            Codec::H265 => ("h265parse", "video/x-h265"),
        };
        let description =
            format!("appsrc name=source format=time ! {parser} ! mp4mux ! filesink name=sink");
        Self::open(&description, caps, path)
    }

    fn open(description: &str, caps: &str, path: &Path) -> Result<Self, Error> {
        let failed = |error: gst::glib::Error| Error::Write(error.to_string());
        let pipeline = gst::parse::launch(description)
            .map_err(failed)?
            .downcast::<gst::Pipeline>()
            .map_err(|_| Error::Write("not a pipeline".to_owned()))?;
        let element = |name: &str| pipeline.by_name(name).expect("named in the description");
        element("sink").set_property("location", path.to_string_lossy().as_ref());
        let source = element("source")
            .downcast::<AppSrc>()
            .expect("created as appsrc");
        source.set_caps(Some(
            &gst::Caps::builder(caps)
                .field("stream-format", "byte-stream")
                .field("alignment", "au")
                .build(),
        ));
        let muxer = Self { pipeline, source };
        muxer
            .pipeline
            .set_state(gst::State::Playing)
            .map_err(|_| muxer.failure())?;
        Ok(muxer)
    }

    fn push(&self, frame: Frame, cancel: &AtomicBool) -> Result<(), Error> {
        self.wait_for_room(cancel)?;
        let milliseconds = frame.elapsed_ms;
        self.source
            .push_buffer(buffer_of(frame, milliseconds))
            .map_err(|_| self.failure())?;
        Ok(())
    }

    /// `block=true` on `appsrc` would wait in a place that only ends when the pipeline stops, so
    /// a full disk would hold the export forever, deaf to a cancellation.
    fn wait_for_room(&self, cancel: &AtomicBool) -> Result<(), Error> {
        let bus = self.pipeline.bus().expect("pipelines have a bus");
        loop {
            if let Some(message) = bus.pop_filtered(&[gst::MessageType::Error]) {
                return Err(write_error(&message));
            }
            if self.source.current_level_bytes() < MAX_QUEUED_BYTES {
                return Ok(());
            }
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            std::thread::sleep(QUEUE_CHECK_INTERVAL);
        }
    }

    /// Writes the index of the MP4, without which the file can't be played.
    fn finish(self) -> Result<(), Error> {
        self.source.end_of_stream().map_err(|_| self.failure())?;
        let bus = self.pipeline.bus().expect("pipelines have a bus");
        let message = bus.timed_pop_filtered(
            gst::ClockTime::from_seconds(60),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        );
        match &message {
            Some(message) if matches!(message.view(), gst::MessageView::Eos(_)) => Ok(()),
            Some(message) => Err(write_error(message)),
            None => Err(Error::Write("timed out".to_owned())),
        }
    }

    fn failure(&self) -> Error {
        let bus = self.pipeline.bus().expect("pipelines have a bus");
        match bus.pop_filtered(&[gst::MessageType::Error]) {
            Some(message) => write_error(&message),
            None => Error::Write("the video could not be written".to_owned()),
        }
    }
}

fn write_error(message: &gst::Message) -> Error {
    match message.view() {
        gst::MessageView::Error(error) => Error::Write(error.error().to_string()),
        _ => Error::Write("the video could not be written".to_owned()),
    }
}

impl Drop for Muxer {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

/// Removes an unfinished file, which has no index and doesn't play.
struct Partial<'a>(&'a Path);

impl Drop for Partial<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0);
    }
}

fn write(
    client: Arc<Client>,
    job: &Job,
    path: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f64),
) -> Result<(), Error> {
    let start = job.start.saturating_sub(MARGIN_SECONDS);
    let end = (job.end + MARGIN_SECONDS).min(LAST_SECOND_OF_DAY);
    let pieces = download(client, job.channel, job.date.clone(), start, end, true);

    let expected_ms = f64::from(end - start) * 1000.0;
    let mut reader = Reader::new();
    let mut muxer = None;
    let mut waiting = Duration::ZERO;
    let mut reported = Instant::now();
    let mut last_time = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let piece = match pieces.recv_timeout(CHECK_INTERVAL) {
            Ok(piece) => piece?,
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                waiting += CHECK_INTERVAL;
                if waiting >= STALL_LIMIT {
                    return Err(Error::Stopped);
                }
                continue;
            }
        };
        waiting = Duration::ZERO;
        for frame in reader.push(&piece) {
            last_time = frame.elapsed_ms;
            feed(&mut muxer, frame, path, cancel)?;
        }
        if reported.elapsed() >= PROGRESS_INTERVAL {
            reported = Instant::now();
            progress((last_time as f64 / expected_ms).min(1.0));
        }
    }
    for frame in reader.finish() {
        feed(&mut muxer, frame, path, cancel)?;
    }
    muxer.ok_or(Error::Empty)?.finish()
}

fn feed(
    muxer: &mut Option<Muxer>,
    frame: Frame,
    path: &Path,
    cancel: &AtomicBool,
) -> Result<(), Error> {
    if muxer.is_none() {
        // The first frame out of the reader is always a key frame.
        *muxer = Some(Muxer::new(frame.codec(), path)?);
    }
    muxer.as_ref().expect("just created").push(frame, cancel)
}

/// Saves a stretch and returns its path. Only a finished video gets the final name, so a file
/// without the `.part` suffix always plays.
pub fn run(
    client: Arc<Client>,
    job: &Job,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f64),
) -> Result<PathBuf, Error> {
    let missing = missing_from(&ELEMENTS);
    if let Some(element) = missing.first() {
        return Err(Error::Missing((*element).to_owned()));
    }
    let folder = clips::folder();
    std::fs::create_dir_all(&folder).map_err(|error| Error::Write(error.to_string()))?;
    let stem = clips::file_stem(&job.name, &job.date, job.start, job.end);
    let partial = folder.join(format!(".{stem}.{}.part", std::process::id()));
    let guard = Partial(&partial);
    write(client, job, &partial, cancel, progress)?;
    let path = clips::free_path(&folder, &stem);
    std::fs::rename(&partial, &path).map_err(|error| Error::Write(error.to_string()))?;
    drop(guard);
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Frame {
        Frame {
            key: true,
            elapsed_ms: 0,
            data: vec![0; 1024 * 1024],
        }
    }

    fn muxer(pipeline: &str, path: &str) -> Muxer {
        gst::init().unwrap();
        Muxer::open(pipeline, "video/x-h264", Path::new(path)).unwrap()
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_full_disk_stops_the_export_with_its_error() {
        let muxer = muxer(
            "appsrc name=source format=time ! filesink name=sink",
            "/dev/full",
        );
        let cancel = AtomicBool::new(false);
        let result = (0..1_000).try_for_each(|_| muxer.push(frame(), &cancel));
        assert!(matches!(result, Err(Error::Write(_))), "{result:?}");
    }

    #[test]
    fn a_cancellation_gets_out_of_a_full_queue() {
        // Each buffer takes 1 second, so the queue fills and stays full.
        let muxer = muxer(
            "appsrc name=source format=time ! identity sleep-time=1000000 \
             ! filesink name=sink",
            std::env::temp_dir()
                .join(format!("rigelcam-test-{}", std::process::id()))
                .to_str()
                .unwrap(),
        );
        let cancel = AtomicBool::new(true);
        let result = (0..100).try_for_each(|_| muxer.push(frame(), &cancel));
        assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
        drop(muxer);
        let _ = std::fs::remove_file(
            std::env::temp_dir().join(format!("rigelcam-test-{}", std::process::id())),
        );
    }
}
