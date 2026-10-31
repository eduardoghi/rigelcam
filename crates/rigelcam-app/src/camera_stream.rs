//! RTSP pipeline of a camera, which hands DMABuf frames to a video item and reconnects by itself.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_allocators::DmaBufMemory;
use gstreamer_app::{AppSink, AppSinkCallbacks};
use gstreamer_video::{VideoColorMatrix, VideoColorRange, VideoColorimetry, VideoMeta};

/// Without these no camera opens, whatever the recorder sends.
const REQUIRED_ELEMENTS: [&str; 4] = ["rtspsrc", "rtph264depay", "h264parse", "vah264dec"];
const H265_ELEMENTS: [&str; 3] = ["rtph265depay", "h265parse", "vah265dec"];
const DRM_FORMAT_NV12: u32 = 0x3231_564e;
/// A stream that stops sending frames without an error counts as lost after this long.
const STALL_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);
/// How often a recording tells where it is, which is enough for a moving bar.
const POSITION_INTERVAL: Duration = Duration::from_millis(250);

#[repr(C)]
struct RigelFrame {
    width: i32,
    height: i32,
    fourcc: u32,
    modifier: u64,
    generation: u64,
    n_planes: i32,
    full_range: i32,
    matrix: i32,
    fds: [i32; 4],
    offsets: [u32; 4],
    strides: [u32; 4],
    owner: *mut c_void,
    release: extern "C" fn(*mut c_void),
}

extern "C" {
    fn rigel_item_set_frame(item: *mut c_void, frame: *const RigelFrame);
    fn rigel_item_set_status(item: *mut c_void, has_video: bool, signal_lost: bool);
    fn rigel_item_auth_refused(item: *mut c_void);
    fn rigel_item_position(item: *mut c_void, seconds: f64);
    fn rigel_item_ended(item: *mut c_void);
}

extern "C" fn release_sample(owner: *mut c_void) {
    drop(unsafe { Box::from_raw(owner as *mut gst::Sample) });
}

#[derive(Clone, PartialEq, Eq)]
pub struct Source {
    pub url: String,
    pub user: String,
    pub password: String,
    /// A recording, which reports its position and doesn't reconnect, since starting over would
    /// go back to where it was opened.
    pub recording: bool,
    /// Speed of a recording, in times the normal one.
    pub rate: u32,
}

#[derive(Default)]
struct Control {
    source: Option<Source>,
    paused: bool,
    /// Bumped on every change, so the worker knows its pipeline is out of date.
    revision: u64,
    stop: bool,
}

struct Shared {
    control: Mutex<Control>,
    changed: Condvar,
    /// Address of the video item, or 0 once it is gone. Calls into the item hold this lock, so the
    /// item can't be destroyed in the middle of one.
    item: Mutex<usize>,
}

impl Shared {
    fn set_status(&self, has_video: bool, signal_lost: bool) {
        let item = self.item.lock().unwrap();
        if *item != 0 {
            unsafe { rigel_item_set_status(*item as *mut c_void, has_video, signal_lost) };
        }
    }

    fn report_position(&self, seconds: f64) {
        let item = self.item.lock().unwrap();
        if *item != 0 {
            unsafe { rigel_item_position(*item as *mut c_void, seconds) };
        }
    }

    fn report_ended(&self) {
        let item = self.item.lock().unwrap();
        if *item != 0 {
            unsafe { rigel_item_ended(*item as *mut c_void) };
        }
    }

    fn report_refused(&self) {
        let item = self.item.lock().unwrap();
        if *item != 0 {
            unsafe { rigel_item_auth_refused(*item as *mut c_void) };
        }
    }
}

fn missing_from<'a>(elements: &[&'a str]) -> Vec<&'a str> {
    elements
        .iter()
        .copied()
        .filter(|name| gst::ElementFactory::find(name).is_none())
        .collect()
}

/// GStreamer elements this system lacks to show video.
pub fn missing_elements() -> Vec<&'static str> {
    missing_from(&REQUIRED_ELEMENTS)
}

/// GStreamer elements this system lacks to show H.265 cameras, which H.264 cameras don't need.
pub fn missing_h265_elements() -> Vec<&'static str> {
    missing_from(&H265_ELEMENTS)
}

/// Owned by the video item, which drops it before going away.
pub struct CameraStream {
    shared: Arc<Shared>,
}

impl CameraStream {
    pub fn new(item: *mut c_void) -> Self {
        let shared = Arc::new(Shared {
            control: Mutex::default(),
            changed: Condvar::new(),
            item: Mutex::new(item as usize),
        });
        let worker_shared = shared.clone();
        std::thread::Builder::new()
            .name("camera".into())
            .spawn(move || run(worker_shared))
            .expect("could not start the camera thread");
        Self { shared }
    }

    /// Plays `source`, or stops receiving video with `None`.
    pub fn set_source(&self, source: Option<Source>) {
        let mut control = self.shared.control.lock().unwrap();
        // The grid asks again for what is already playing when it shows a hidden camera, and a
        // new revision would reopen a stream that is working.
        if control.source == source {
            return;
        }
        control.source = source;
        control.revision += 1;
        self.shared.changed.notify_all();
    }
}

impl CameraStream {
    pub fn set_paused(&self, paused: bool) {
        self.shared.control.lock().unwrap().paused = paused;
    }
}

impl Drop for CameraStream {
    // Doesn't wait for the worker, since closing an RTSP session can take seconds and this runs on
    // the interface thread. After this the worker can't reach the item anymore.
    fn drop(&mut self) {
        *self.shared.item.lock().unwrap() = 0;
        self.shared.control.lock().unwrap().stop = true;
        self.shared.changed.notify_all();
    }
}

fn run(shared: Arc<Shared>) {
    let mut generation = 0;
    let mut retry_delay = Duration::from_secs(2);
    // A camera that stays offline fails on every retry, so only a new reason reaches the log.
    let mut last_reason = String::new();
    loop {
        let (source, revision) = {
            let guard = shared.control.lock().unwrap();
            let control = shared
                .changed
                .wait_while(guard, |c| !c.stop && c.source.is_none())
                .unwrap();
            if control.stop {
                return;
            }
            (control.source.clone().unwrap(), control.revision)
        };
        generation += 1;
        let result = play(&shared, &source, revision, generation);
        match result {
            Outcome::Changed => {
                retry_delay = Duration::from_secs(2);
                last_reason.clear();
                // With a new source, the current picture stays until its first frame arrives.
                if shared.control.lock().unwrap().source.is_none() {
                    shared.set_status(false, false);
                }
            }
            // Repeating a refused password can lock the user on the recorder, so this stream waits
            // for a new source, and the grid stops the other cameras of the recorder.
            Outcome::Refused => {
                eprintln!("the recorder refused the login for {}", source.url);
                shared.set_status(false, true);
                shared.report_refused();
                let guard = shared.control.lock().unwrap();
                drop(
                    shared
                        .changed
                        .wait_while(guard, |c| !c.stop && c.revision == revision)
                        .unwrap(),
                );
            }
            Outcome::Lost {
                received_video,
                reason,
            } => {
                shared.set_status(false, true);
                if source.recording {
                    eprintln!("{} ({reason})", source.url);
                    shared.report_ended();
                    let guard = shared.control.lock().unwrap();
                    drop(
                        shared
                            .changed
                            .wait_while(guard, |c| !c.stop && c.revision == revision)
                            .unwrap(),
                    );
                    continue;
                }
                if received_video {
                    retry_delay = Duration::from_secs(2);
                    last_reason.clear();
                }
                if reason != last_reason {
                    eprintln!("{} ({reason})", source.url);
                    last_reason = reason;
                }
                let guard = shared.control.lock().unwrap();
                let _ = shared
                    .changed
                    .wait_timeout_while(guard, retry_delay, |c| !c.stop && c.revision == revision)
                    .unwrap();
                retry_delay = (retry_delay * 2).min(MAX_RETRY_DELAY);
            }
        }
    }
}

enum Outcome {
    /// The source changed or the stream was asked to stop.
    Changed,
    Refused,
    Lost {
        received_video: bool,
        reason: String,
    },
}

fn play(shared: &Arc<Shared>, source: &Source, revision: u64, generation: u64) -> Outcome {
    let last_frame = Arc::new(Mutex::new(Instant::now()));
    let received_video = Arc::new(AtomicBool::new(false));
    let waiting_keyframe = Arc::new(AtomicBool::new(true));
    let Some(pipeline) = build_pipeline(
        shared.clone(),
        source,
        generation,
        last_frame.clone(),
        received_video.clone(),
        waiting_keyframe.clone(),
    ) else {
        return Outcome::Lost {
            received_video: false,
            reason: "could not build the pipeline".to_owned(),
        };
    };
    let bus = pipeline.bus().unwrap();
    let mut paused = false;
    let mut speed_applied = source.rate <= 1;
    let outcome = if pipeline.set_state(gst::State::Playing).is_err() {
        Outcome::Lost {
            received_video: false,
            reason: "could not start the pipeline".to_owned(),
        }
    } else {
        loop {
            {
                let control = shared.control.lock().unwrap();
                if control.stop || control.revision != revision {
                    break Outcome::Changed;
                }
                if control.paused != paused {
                    paused = control.paused;
                    let state = if paused {
                        gst::State::Paused
                    } else {
                        gst::State::Playing
                    };
                    let _ = pipeline.set_state(state);
                }
            }
            if paused {
                *last_frame.lock().unwrap() = Instant::now();
            }
            // Only once frames flow, because rtspsrc can't seek before it is playing.
            if !speed_applied && received_video.load(Ordering::Relaxed) {
                speed_applied = true;
                waiting_keyframe.store(true, Ordering::Relaxed);
                let seek = pipeline.seek(
                    f64::from(source.rate),
                    gst::SeekFlags::FLUSH,
                    gst::SeekType::Set,
                    gst::ClockTime::ZERO,
                    gst::SeekType::None,
                    gst::ClockTime::NONE,
                );
                if let Err(error) = seek {
                    eprintln!("could not change the speed of {} ({error})", source.url);
                }
                // The seek flushed the decoder, and buffers may have passed before it did.
                waiting_keyframe.store(true, Ordering::Relaxed);
            }
            if let Some(message) = bus.timed_pop(gst::ClockTime::from_mseconds(200)) {
                match message.view() {
                    gst::MessageView::Error(error) => {
                        if error.error().matches(gst::ResourceError::NotAuthorized) {
                            break Outcome::Refused;
                        }
                        break Outcome::Lost {
                            received_video: received_video.load(Ordering::Relaxed),
                            reason: error.error().to_string(),
                        };
                    }
                    gst::MessageView::Eos(_) => {
                        break Outcome::Lost {
                            received_video: received_video.load(Ordering::Relaxed),
                            reason: "the stream ended".to_owned(),
                        };
                    }
                    _ => {}
                }
            }
            if last_frame.lock().unwrap().elapsed() > STALL_TIMEOUT {
                break Outcome::Lost {
                    received_video: received_video.load(Ordering::Relaxed),
                    reason: format!("no frames for {} s", STALL_TIMEOUT.as_secs()),
                };
            }
        }
    };
    let _ = pipeline.set_state(gst::State::Null);
    outcome
}

fn build_pipeline(
    shared: Arc<Shared>,
    source: &Source,
    generation: u64,
    last_frame: Arc<Mutex<Instant>>,
    received_video: Arc<AtomicBool>,
    waiting_keyframe: Arc<AtomicBool>,
) -> Option<gst::Pipeline> {
    let pipeline = gst::Pipeline::new();
    let src = gst::ElementFactory::make("rtspsrc")
        .property("location", source.url.as_str())
        .property_from_str("protocols", "tcp")
        .property("latency", 300u32)
        // Through properties, so the password stays out of the URL and of the logs.
        .property("user-id", source.user.as_str())
        .property("user-pw", source.password.as_str())
        .build()
        .ok()?;
    pipeline.add(&src).ok()?;

    src.connect("select-stream", false, |values| {
        let media = values[2].get::<gst::Caps>().ok().and_then(|caps| {
            let structure = caps.structure(0)?;
            structure.get::<String>("media").ok()
        });
        // The signal has to return a boolean, or GLib aborts.
        Some((media.as_deref() == Some("video")).to_value())
    });

    let recording = source.recording;
    let weak_pipeline = pipeline.downgrade();
    src.connect_pad_added(move |_, pad| {
        let Some(pipeline) = weak_pipeline.upgrade() else {
            return;
        };
        let Some(caps) = pad.current_caps() else {
            return;
        };
        let Some(structure) = caps.structure(0) else {
            return;
        };
        let encoding = structure.get::<String>("encoding-name").unwrap_or_default();
        let (depay, parse, decoder) = match encoding.as_str() {
            "H264" => ("rtph264depay", "h264parse", "vah264dec"),
            "H265" => ("rtph265depay", "h265parse", "vah265dec"),
            other => {
                fail(&pipeline, &format!("unsupported encoding ({other})"));
                return;
            }
        };
        let description = format!(
            "{depay} ! {parse} name=parser ! {decoder} ! video/x-raw(memory:DMABuf),format=DMA_DRM \
             ! appsink name=sink sync=false max-buffers=1 drop=true"
        );
        let Ok(branch) = gst::parse::bin_from_description(&description, true) else {
            fail(&pipeline, "could not build the decoding branch");
            return;
        };
        let sink = branch
            .by_name("sink")
            .unwrap()
            .downcast::<AppSink>()
            .unwrap();
        // The decoder refuses DMABuf unless downstream declares VideoMeta, which appsink doesn't.
        sink.static_pad("sink").unwrap().add_probe(
            gst::PadProbeType::QUERY_DOWNSTREAM,
            |_, info| {
                if let Some(query) = info.query_mut() {
                    if let gst::QueryViewMut::Allocation(allocation) = query.view_mut() {
                        allocation.add_allocation_meta::<VideoMeta>(None);
                    }
                }
                gst::PadProbeReturn::Ok
            },
        );
        // A frame that depends on a keyframe the decoder doesn't have comes out green, which is
        // what the stream starts with when it opens or changes speed between two keyframes.
        let waiting = waiting_keyframe.clone();
        let parser = branch.by_name("parser").unwrap();
        parser
            .static_pad("src")
            .unwrap()
            .add_probe(gst::PadProbeType::BUFFER, move |_, info| {
                let Some(gst::PadProbeData::Buffer(buffer)) = &info.data else {
                    return gst::PadProbeReturn::Ok;
                };
                if !waiting.load(Ordering::Relaxed) {
                    return gst::PadProbeReturn::Ok;
                }
                if buffer.flags().contains(gst::BufferFlags::DELTA_UNIT) {
                    return gst::PadProbeReturn::Drop;
                }
                waiting.store(false, Ordering::Relaxed);
                gst::PadProbeReturn::Ok
            });
        let last_frame = last_frame.clone();
        let received_video = received_video.clone();
        let shared = shared.clone();
        let mut last_report = Instant::now();
        sink.set_callbacks(
            AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    if recording && last_report.elapsed() >= POSITION_INTERVAL {
                        if let Some(seconds) = stream_time_of(&sample) {
                            last_report = Instant::now();
                            shared.report_position(seconds);
                        }
                    }
                    // A frame the item can't take doesn't count, so the stream ends up as lost.
                    if push_sample(&shared, sample, generation).is_some() {
                        *last_frame.lock().unwrap() = Instant::now();
                        if !received_video.swap(true, Ordering::Relaxed) {
                            shared.set_status(true, false);
                        }
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        // A panic here would abort the app, since this runs inside a C callback.
        let started = pipeline.add(&branch).is_ok()
            && branch.sync_state_with_parent().is_ok()
            && branch
                .static_pad("sink")
                .is_some_and(|sink| pad.link(&sink).is_ok());
        if !started {
            fail(&pipeline, "could not start the decoder");
        }
    });
    Some(pipeline)
}

/// Reports a failure from a streaming thread to the loop in `play`, which handles it as a lost stream.
fn fail(pipeline: &gst::Pipeline, reason: &str) {
    let _ = pipeline.post_message(gst::message::Error::new(gst::CoreError::Failed, reason));
}

fn modifier_of(sample: &gst::Sample) -> Option<u64> {
    let caps = sample.caps()?;
    let drm = caps.structure(0)?.get::<String>("drm-format").ok()?;
    let (format, modifier) = drm.split_once(':').unwrap_or((drm.as_str(), "0x0"));
    if format != "NV12" {
        return None;
    }
    u64::from_str_radix(modifier.trim_start_matches("0x"), 16).ok()
}

/// Seconds of recording this frame is at, since the point the stream was opened at.
fn stream_time_of(sample: &gst::Sample) -> Option<f64> {
    let position = sample.buffer()?.pts()?;
    let segment = sample.segment()?.downcast_ref::<gst::ClockTime>()?;
    let time = segment.to_stream_time(position)?;
    Some(time.nseconds() as f64 / 1e9)
}

/// Whether the range is full, and the code of the color matrix for the video item.
fn color_of(sample: &gst::Sample) -> Option<(i32, i32)> {
    let caps = sample.caps()?;
    let text = caps.structure(0)?.get::<String>("colorimetry").ok()?;
    let colorimetry: VideoColorimetry = text.parse().ok()?;
    let matrix = match colorimetry.matrix() {
        VideoColorMatrix::Bt709 => 1,
        VideoColorMatrix::Bt2020 => 2,
        _ => 0,
    };
    Some((
        (colorimetry.range() == VideoColorRange::Range0_255) as i32,
        matrix,
    ))
}

fn push_sample(shared: &Shared, sample: gst::Sample, generation: u64) -> Option<()> {
    let buffer = sample.buffer()?;
    let meta = buffer.meta::<VideoMeta>()?;
    let (full_range, matrix) = color_of(&sample).unwrap_or((0, 0));
    let mut frame = RigelFrame {
        width: meta.width() as i32,
        height: meta.height() as i32,
        fourcc: DRM_FORMAT_NV12,
        modifier: modifier_of(&sample)?,
        generation,
        n_planes: meta.n_planes() as i32,
        full_range,
        matrix,
        fds: [-1; 4],
        offsets: [0; 4],
        strides: [0; 4],
        owner: std::ptr::null_mut(),
        release: release_sample,
    };
    for plane in 0..meta.n_planes() as usize {
        let memory = buffer.peek_memory(plane.min(buffer.n_memory() - 1));
        frame.fds[plane] = memory.downcast_memory_ref::<DmaBufMemory>()?.fd();
        frame.offsets[plane] = meta.offset()[plane] as u32;
        frame.strides[plane] = meta.stride()[plane] as u32;
    }
    let item = shared.item.lock().unwrap();
    if *item == 0 {
        return None;
    }
    frame.owner = Box::into_raw(Box::new(sample)) as *mut c_void;
    unsafe { rigel_item_set_frame(*item as *mut c_void, &frame) };
    Some(())
}
