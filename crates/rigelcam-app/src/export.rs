//! Saves a stretch of a recording as an MP4, with the video as the camera encoded it.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, RecvTimeoutError};
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;
use rigelcam_core::clips;
use rigelcam_core::dahua::{Client, Error as RecorderError};
use rigelcam_core::dav::{Codec, Frame, Reader};

use crate::camera_stream::missing_from;

const ELEMENTS: [&str; 5] = ["appsrc", "h264parse", "h265parse", "mp4mux", "filesink"];
/// Added to each side, since a video can only start at a key frame and the recorder stops up to 2
/// seconds short of the end. A longer video is better than one missing what was asked for.
const MARGIN_SECONDS: u32 = 2;
const LAST_SECOND_OF_DAY: u32 = 86_399;
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
        let description = format!(
            "appsrc name=source format=time block=true max-bytes=33554432 \
             ! {parser} ! mp4mux ! filesink name=sink"
        );
        let failed = |error: gst::glib::Error| Error::Write(error.to_string());
        let pipeline = gst::parse::launch(&description)
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

    fn push(&self, frame: Frame) -> Result<(), Error> {
        let time = gst::ClockTime::from_mseconds(frame.elapsed_ms);
        let mut buffer = gst::Buffer::from_mut_slice(frame.data);
        {
            let buffer = buffer.get_mut().expect("just created");
            buffer.set_pts(time);
            buffer.set_dts(time);
            if !frame.key {
                buffer.set_flags(gst::BufferFlags::DELTA_UNIT);
            }
        }
        self.source
            .push_buffer(buffer)
            .map_err(|_| self.failure())?;
        Ok(())
    }

    /// Writes the index of the MP4, without which the file can't be played.
    fn finish(self) -> Result<(), Error> {
        self.source.end_of_stream().map_err(|_| self.failure())?;
        let bus = self.pipeline.bus().expect("pipelines have a bus");
        let message = bus.timed_pop_filtered(
            gst::ClockTime::from_seconds(60),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        );
        match message.as_ref().map(|message| message.view()) {
            Some(gst::MessageView::Eos(_)) => Ok(()),
            Some(gst::MessageView::Error(error)) => Err(Error::Write(error.error().to_string())),
            _ => Err(Error::Write("timed out".to_owned())),
        }
    }

    fn failure(&self) -> Error {
        let bus = self.pipeline.bus().expect("pipelines have a bus");
        let message = bus.pop_filtered(&[gst::MessageType::Error]);
        match message.as_ref().map(|message| message.view()) {
            Some(gst::MessageView::Error(error)) => Error::Write(error.error().to_string()),
            _ => Error::Write("the video could not be written".to_owned()),
        }
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
    client: &Client,
    job: &Job,
    path: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f64),
) -> Result<(), Error> {
    let start = job.start.saturating_sub(MARGIN_SECONDS);
    let end = (job.end + MARGIN_SECONDS).min(LAST_SECOND_OF_DAY);
    // The recorder answers a stretch with no recordings as a bad request, as its searches do.
    let mut body = client
        .download(job.channel, &job.date, start, end)
        .map_err(|error| match error {
            RecorderError::Status(400) => Error::Empty,
            other => other.into(),
        })?;

    let (sender, pieces) = sync_channel(PIECES_IN_FLIGHT);
    std::thread::spawn(move || loop {
        let mut piece = vec![0; READ_SIZE];
        let read = body.read(&mut piece);
        if let Ok(size) = read {
            piece.truncate(size);
        }
        let done = !matches!(read, Ok(size) if size > 0);
        // Fails when the export gave up on this download, which also closes it.
        if sender.send(read.map(|_| piece)).is_err() || done {
            return;
        }
    });

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
            Ok(piece) => piece.map_err(|_| Error::Stopped)?,
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
        if piece.is_empty() {
            break;
        }
        for frame in reader.push(&piece) {
            last_time = frame.elapsed_ms;
            feed(&mut muxer, frame, path)?;
        }
        if reported.elapsed() >= PROGRESS_INTERVAL {
            reported = Instant::now();
            progress((last_time as f64 / expected_ms).min(1.0));
        }
    }
    for frame in reader.finish() {
        feed(&mut muxer, frame, path)?;
    }
    muxer.ok_or(Error::Empty)?.finish()
}

fn feed(muxer: &mut Option<Muxer>, frame: Frame, path: &Path) -> Result<(), Error> {
    if muxer.is_none() {
        // The first frame out of the reader is always a key frame.
        *muxer = Some(Muxer::new(frame.codec(), path)?);
    }
    muxer.as_ref().expect("just created").push(frame)
}

/// Saves a stretch and returns its path. Only a finished video gets the final name, so a file
/// without the `.part` suffix always plays.
pub fn run(
    client: &Client,
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
