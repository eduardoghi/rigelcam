//! Pipeline of a camera, which hands decoded frames to a video item and reconnects by itself. On
//! Linux they go as DMABuf, and on Windows as Direct3D 11 textures of the decoder. Video comes from
//! the recorder over RTSP. What it answers 404 to while the camera works comes from
//! elsewhere, live video from the camera itself and recordings from the HTTP API.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use rigelcam_core::dahua::{Client, Error as RecorderError, LoginGate};
use rigelcam_core::dav::{Codec, Frame, Reader};

use gstreamer as gst;
use gstreamer::prelude::*;
#[cfg(unix)]
use gstreamer_allocators::DmaBufMemory;
use gstreamer_app::{AppSink, AppSinkCallbacks, AppSrc};
#[cfg(windows)]
use gstreamer_video::VideoInfo;
use gstreamer_video::{VideoColorMatrix, VideoColorRange, VideoColorimetry, VideoMeta};

use crate::export::{self, LAST_SECOND_OF_DAY};

#[cfg(windows)]
const H264_DECODER: &str = "d3d11h264dec";
#[cfg(windows)]
const H265_DECODER: &str = "d3d11h265dec";
/// Frames stay on the GPU, in the memory the video item can read without copying them to the CPU.
#[cfg(windows)]
// P010 is what a decoder gives for 10 bit video, which some H.265 cameras send.
const DECODED_CAPS: &str = "video/x-raw(memory:D3D11Memory),format=(string){NV12,P010_10LE}";
#[cfg(unix)]
const H264_DECODER: &str = "vah264dec";
#[cfg(unix)]
const H265_DECODER: &str = "vah265dec";
#[cfg(unix)]
const DECODED_CAPS: &str = "video/x-raw(memory:DMABuf),format=DMA_DRM";

/// Without these no camera opens, whatever the recorder sends.
const REQUIRED_ELEMENTS: [&str; 4] = ["rtspsrc", "rtph264depay", "h264parse", H264_DECODER];
const H265_ELEMENTS: [&str; 3] = ["rtph265depay", "h265parse", H265_DECODER];
#[cfg(unix)]
const DRM_FORMAT_NV12: u32 = 0x3231_564e;
/// What VA gives for 10 bit video, which some H.265 cameras send.
#[cfg(unix)]
const DRM_FORMAT_P010: u32 = 0x3031_3050;
const RTSP_FORBIDDEN: u32 = 403;
/// A stream that stops sending frames without an error counts as lost after this long.
const STALL_TIMEOUT: Duration = Duration::from_secs(15);
const PERMISSION_TIMEOUT: Duration = Duration::from_secs(10);
/// The first frame also waits for the RTSP handshake and for a key frame.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);
const STREAM_ENDED: &str = "the stream ended";
const NOT_ON_RTSP: &str = "the recorder has no such stream";
/// From this speed only key frames are decoded, which is also what recorders send over RTSP.
const KEY_FRAMES_ONLY_RATE: u32 = 8;
/// How often a thread waiting for video checks whether it should still wait.
const CHECK_INTERVAL: Duration = Duration::from_millis(200);
/// OPTIONS, DESCRIBE and the repeat of whichever of them got the challenge.
const MAX_REQUESTS_BEFORE_LOGIN: u32 = 3;
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
    sample: *mut c_void,
    owner: *mut c_void,
    release: extern "C" fn(*mut c_void),
}

extern "C" {
    fn rigel_item_set_frame(item: *mut c_void, frame: *const RigelFrame);
    #[cfg(windows)]
    fn rigel_decoder_context() -> *mut gst::ffi::GstContext;
    fn rigel_item_set_status(item: *mut c_void, has_video: bool, signal_lost: bool);
    fn rigel_item_auth_refused(item: *mut c_void);
    fn rigel_item_denied(item: *mut c_void);
    fn rigel_item_position(item: *mut c_void, seconds: f64, attempt: i32);
    fn rigel_item_ended(item: *mut c_void, attempt: i32);
    fn rigel_item_failed(item: *mut c_void, attempt: i32);
}

/// Gives back the reference that `hand_over` passed to the item.
extern "C" fn release_sample(owner: *mut c_void) {
    unsafe { gst::ffi::gst_mini_object_unref(owner as *mut gst::ffi::GstMiniObject) };
}

/// Passes the item the reference of the sample itself, which `release_sample` gives back, since the
/// sample already counts its references and a box around it would be one allocation per frame.
fn hand_over(sample: gst::Sample) -> *mut c_void {
    use gst::glib::translate::IntoGlibPtr;
    let raw: *mut gst::ffi::GstSample = sample.into_glib_ptr();
    raw as *mut c_void
}

/// A recording as a stretch of the HTTP API, for channels that answer 404 to it over RTSP while
/// the camera works and records.
pub struct Stretch {
    pub client: Arc<Client>,
    pub channel: u32,
    pub date: String,
    /// Seconds after the midnight of `date`.
    pub start: u32,
    pub without_rtsp: Arc<Mutex<HashSet<u32>>>,
    /// Channels an export found allowed, which a denied playback takes out.
    pub allowed: Arc<Mutex<HashMap<u32, Instant>>>,
}

/// Channels of a recorder whose live video comes from the camera.
#[derive(Default)]
pub struct DirectCameras {
    opened: Mutex<HashSet<u32>>,
    /// Channels the recorder sent video for, whose 404 only means their camera dropped for a moment.
    /// Those cameras usually have a password of their own, so going to them would be a wrong login.
    served: Mutex<HashSet<u32>>,
    /// Cameras with a password other than the one of the recorder. Each try is a wrong password,
    /// and those can lock the user on the camera.
    refused: Mutex<HashSet<u32>>,
}

/// The camera to go to if the recorder answers 404 to its live video.
pub struct Direct {
    pub url: String,
    pub channel: u32,
    pub cameras: Arc<DirectCameras>,
    pub client: Arc<Client>,
}

#[derive(Clone)]
pub struct Source {
    pub url: String,
    pub user: String,
    pub password: String,
    /// A recording, which reports its position and doesn't reconnect, since starting over would
    /// go back to where it was opened.
    pub recording: bool,
    /// Speed of a recording, in times the normal one.
    pub rate: u32,
    /// Number of this opening of a recording. It makes every one a new source, so asking again
    /// for the same moment reopens a recording that failed or ended.
    pub attempt: i32,
    pub gate: Arc<LoginGate>,
    /// Only on a recording.
    pub stretch: Option<Arc<Stretch>>,
    /// Only on live video of a channel whose camera the recorder told about.
    pub direct: Option<Arc<Direct>>,
}

// The gate belongs to the recorder, and it comes along with these from the same connection.
impl PartialEq for Source {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
            && self.user == other.user
            && self.password == other.password
            && self.recording == other.recording
            && self.rate == other.rate
            && self.attempt == other.attempt
    }
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
    /// The bus of the pipeline playing now, where `wake` reaches the worker waiting on it.
    bus: Mutex<Option<gst::Bus>>,
}

impl Shared {
    /// Wakes the worker while it watches a pipeline, after a change it has to look at.
    fn wake(&self) {
        if let Some(bus) = &*self.bus.lock().unwrap() {
            let _ = bus.post(gst::message::Application::new(gst::Structure::new_empty(
                "wake",
            )));
        }
    }
}

impl Shared {
    fn set_status(&self, has_video: bool, signal_lost: bool) {
        let item = self.item.lock().unwrap();
        if *item != 0 {
            unsafe { rigel_item_set_status(*item as *mut c_void, has_video, signal_lost) };
        }
    }

    fn report_position(&self, seconds: f64, attempt: i32) {
        let item = self.item.lock().unwrap();
        if *item != 0 {
            unsafe { rigel_item_position(*item as *mut c_void, seconds, attempt) };
        }
    }

    fn report_ended(&self, cleanly: bool, attempt: i32) {
        let item = self.item.lock().unwrap();
        if *item != 0 {
            let item = *item as *mut c_void;
            unsafe {
                if cleanly {
                    rigel_item_ended(item, attempt);
                } else {
                    rigel_item_failed(item, attempt);
                }
            }
        }
    }

    fn report_refused(&self) {
        let item = self.item.lock().unwrap();
        if *item != 0 {
            unsafe { rigel_item_auth_refused(*item as *mut c_void) };
        }
    }

    fn report_denied(&self) {
        let item = self.item.lock().unwrap();
        if *item != 0 {
            unsafe { rigel_item_denied(*item as *mut c_void) };
        }
    }
}

pub(crate) fn missing_from<'a>(elements: &[&'a str]) -> Vec<&'a str> {
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

/// The GStreamer version of this system, if it is too old for the decoded caps on Linux.
#[cfg(unix)]
pub fn old_gstreamer() -> Option<String> {
    let (major, minor, micro, _) = gst::version();
    ((major, minor) < (1, 24)).then(|| format!("{major}.{minor}.{micro}"))
}

#[cfg(windows)]
pub fn old_gstreamer() -> Option<String> {
    None
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
            bus: Mutex::default(),
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
        drop(control);
        self.shared.changed.notify_all();
        self.shared.wake();
    }
}

impl CameraStream {
    pub fn set_paused(&self, paused: bool) {
        self.shared.control.lock().unwrap().paused = paused;
        self.shared.wake();
    }
}

impl Drop for CameraStream {
    // Doesn't wait for the worker, since closing an RTSP session can take seconds and this runs on
    // the interface thread. After this the worker can't reach the item anymore.
    fn drop(&mut self) {
        *self.shared.item.lock().unwrap() = 0;
        self.shared.control.lock().unwrap().stop = true;
        self.shared.changed.notify_all();
        self.shared.wake();
    }
}

fn run(shared: Arc<Shared>) {
    let mut generation = 0;
    let mut retry_delay = Duration::from_secs(2);
    // A camera that stays offline fails on every retry, so only a new reason reaches the log.
    let mut last_reason = String::new();
    // A recorder can answer 404 once and open the camera on the next try, so a source that has not
    // shown a picture reads as connecting until it fails again.
    let mut failed_revision = None;
    loop {
        // Also after an end or a failure, when the item can let go of its last frames.
        let idle = shared.control.lock().unwrap().source.is_none();
        if idle {
            shared.set_status(false, false);
        }
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
        let result = if source.gate.allows() {
            let failed_before = failed_revision == Some(revision);
            open(&shared, &source, revision, generation, failed_before)
        } else {
            Outcome::Refused
        };
        match result {
            Outcome::Changed => {
                // With a new source, the current picture stays until its first frame arrives.
                retry_delay = Duration::from_secs(2);
                last_reason.clear();
            }
            // Repeating a refused password can lock the user on the recorder, so this stream waits
            // for a new source, and the grid stops the other cameras of the recorder.
            Outcome::Refused => {
                eprintln!("the recorder refused the login for {}", source.url);
                source.gate.refuse();
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
            // The user has no permission on this channel, which trying again doesn't change and
            // only adds logins to the log of the recorder. Its other channels go on.
            Outcome::Denied => {
                eprintln!("the recorder denied {}", source.url);
                if let Some(stretch) = &source.stretch {
                    stretch.allowed.lock().unwrap().remove(&stretch.channel);
                }
                shared.set_status(false, true);
                shared.report_denied();
                if source.recording {
                    shared.report_ended(false, source.attempt);
                }
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
                shared.set_status(false, received_video || failed_revision == Some(revision));
                failed_revision = Some(revision);
                if source.recording {
                    eprintln!("{} ({reason})", source.url);
                    shared.report_ended(received_video && reason == STREAM_ENDED, source.attempt);
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
    /// The recorder knows the user and refused this channel.
    Denied,
    /// The source changed or the stream was asked to stop.
    Changed,
    Refused,
    Lost {
        received_video: bool,
        reason: String,
    },
}

/// What the threads of a stream know about its video.
#[derive(Clone)]
struct Progress {
    /// Milliseconds after `started` of the last frame, apart from a lock since every frame sets it.
    started: Instant,
    last_frame: Arc<AtomicU64>,
    received_video: Arc<AtomicBool>,
    waiting_keyframe: Arc<AtomicBool>,
}

impl Progress {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            last_frame: Arc::new(AtomicU64::new(0)),
            received_video: Arc::new(AtomicBool::new(false)),
            waiting_keyframe: Arc::new(AtomicBool::new(true)),
        }
    }
}

impl Progress {
    fn saw_frame(&self) {
        let now = self.started.elapsed().as_millis() as u64;
        self.last_frame.store(now, Ordering::Relaxed);
    }

    fn since_last_frame(&self) -> Duration {
        let now = self.started.elapsed().as_millis() as u64;
        Duration::from_millis(now.saturating_sub(self.last_frame.load(Ordering::Relaxed)))
    }
}

/// The status of the RTSP answer behind an error of `rtspsrc`. Everything but 401 and 404 comes
/// as the same generic error, with the status only here.
fn rtsp_status(error: &gst::message::Error) -> Option<u32> {
    error.details()?.get("rtsp-status-code").ok()
}

fn lost(reason: &str) -> Outcome {
    Outcome::Lost {
        received_video: false,
        reason: reason.to_owned(),
    }
}

fn open(
    shared: &Arc<Shared>,
    source: &Source,
    revision: u64,
    generation: u64,
    failed_before: bool,
) -> Outcome {
    if let Some(direct) = &source.direct {
        return open_live(shared, source, direct, revision, generation, failed_before);
    }
    let Some(stretch) = &source.stretch else {
        return play(shared, source, revision, generation);
    };
    if !stretch
        .without_rtsp
        .lock()
        .unwrap()
        .contains(&stretch.channel)
    {
        let outcome = play(shared, source, revision, generation);
        if !matches!(&outcome, Outcome::Lost { reason, .. } if reason == NOT_ON_RTSP) {
            return outcome;
        }
    }
    play_stretch(shared, source, stretch, revision, generation)
}

/// Opens a live stream from the recorder or, for a channel it keeps answering 404 to, from the
/// camera. A camera that dropped from the recorder also gets a 404, and its password is usually
/// another one, so the password only goes to a camera the recorder says is connected.
fn open_live(
    shared: &Arc<Shared>,
    source: &Source,
    direct: &Direct,
    revision: u64,
    generation: u64,
    failed_before: bool,
) -> Outcome {
    let cameras = &direct.cameras;
    if cameras.refused.lock().unwrap().contains(&direct.channel) {
        return play(shared, source, revision, generation);
    }
    if !cameras.opened.lock().unwrap().contains(&direct.channel) {
        let outcome = play(shared, source, revision, generation);
        let not_found = matches!(&outcome, Outcome::Lost { reason, .. } if reason == NOT_ON_RTSP);
        let served = cameras.served.lock().unwrap().contains(&direct.channel);
        if !not_found || !failed_before || served {
            return outcome;
        }
        if direct.client.camera_connected(direct.channel) != Some(true) {
            return outcome;
        }
    }
    cameras.opened.lock().unwrap().insert(direct.channel);
    let camera = Source {
        url: direct.url.clone(),
        direct: None,
        ..source.clone()
    };
    match play(shared, &camera, revision, generation) {
        // Not a refusal of the recorder, whose other channels go on.
        Outcome::Refused => {
            cameras.refused.lock().unwrap().insert(direct.channel);
            lost("the camera refused the password of the recorder")
        }
        outcome => outcome,
    }
}

fn play(shared: &Arc<Shared>, source: &Source, revision: u64, generation: u64) -> Outcome {
    let progress = Progress::new();
    let Some(pipeline) = build_pipeline(shared.clone(), source, generation, progress.clone())
    else {
        return lost("could not build the pipeline");
    };
    watch(
        shared,
        source,
        &pipeline,
        revision,
        &progress,
        source.rate <= 1,
    )
}

/// Runs a pipeline until it ends, fails or stops being wanted.
fn watch(
    shared: &Arc<Shared>,
    source: &Source,
    pipeline: &gst::Pipeline,
    revision: u64,
    progress: &Progress,
    mut speed_applied: bool,
) -> Outcome {
    let Progress {
        received_video,
        waiting_keyframe,
        ..
    } = progress;
    let bus = pipeline.bus().unwrap();
    // Before the first look at the controls, so a change made after it also posts to this bus.
    *shared.bus.lock().unwrap() = Some(bus.clone());
    let mut paused = false;
    let outcome = if pipeline.set_state(gst::State::Playing).is_err() {
        lost("could not start the pipeline")
    } else {
        loop {
            let wanted = {
                let control = shared.control.lock().unwrap();
                if control.stop || control.revision != revision {
                    break Outcome::Changed;
                }
                control.paused
            };
            // Outside the lock, which the interface thread takes to change the source.
            if wanted != paused {
                paused = wanted;
                let state = if paused {
                    gst::State::Paused
                } else {
                    gst::State::Playing
                };
                let _ = pipeline.set_state(state);
                // A pause isn't a stall, so the limit counts again from when it ends.
                progress.saw_frame();
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
            // Only messages wake this, including the ones `wake` posts after a change of the
            // controls or the first frame, so it waits up to the limit without frames, or with no
            // limit while paused.
            let on_video = received_video.load(Ordering::Relaxed);
            let limit = if on_video {
                STALL_TIMEOUT
            } else {
                FIRST_FRAME_TIMEOUT
            };
            let timeout = if paused {
                gst::ClockTime::NONE
            } else {
                let left = limit.saturating_sub(progress.since_last_frame());
                Some(gst::ClockTime::from_mseconds(left.as_millis() as u64 + 1))
            };
            if let Some(message) = bus.timed_pop(timeout) {
                match message.view() {
                    gst::MessageView::Error(error) => {
                        if error.error().matches(gst::ResourceError::NotAuthorized) {
                            break Outcome::Refused;
                        }
                        if rtsp_status(error) == Some(RTSP_FORBIDDEN) {
                            break Outcome::Denied;
                        }
                        if (source.stretch.is_some() || source.direct.is_some())
                            && !received_video.load(Ordering::Relaxed)
                            && error.error().matches(gst::ResourceError::NotFound)
                        {
                            break lost(NOT_ON_RTSP);
                        }
                        break Outcome::Lost {
                            received_video: received_video.load(Ordering::Relaxed),
                            reason: error.error().to_string(),
                        };
                    }
                    gst::MessageView::Eos(_) => {
                        break Outcome::Lost {
                            received_video: received_video.load(Ordering::Relaxed),
                            reason: STREAM_ENDED.to_owned(),
                        };
                    }
                    _ => {}
                }
            }
            if !paused && progress.since_last_frame() > limit {
                break Outcome::Lost {
                    received_video: on_video,
                    reason: format!("no frames for {} s", limit.as_secs()),
                };
            }
        }
    };
    *shared.bus.lock().unwrap() = None;
    let _ = pipeline.set_state(gst::State::Null);
    // Only the recorder sets this, since the stream that opens a camera has no `direct`.
    if let Some(direct) = source
        .direct
        .as_ref()
        .filter(|_| received_video.load(Ordering::Relaxed))
    {
        direct.cameras.served.lock().unwrap().insert(direct.channel);
    }
    outcome
}

/// Plays a recording from the HTTP API. The recorder sends it about 10 times faster than real time
/// and this pipeline shows it on time, so reading waits for the video to need more, also when
/// paused. Each frame counts by the elapsed time of the reader, which skips the gaps as RTSP does.
fn play_stretch(
    shared: &Arc<Shared>,
    source: &Source,
    stretch: &Stretch,
    revision: u64,
    generation: u64,
) -> Outcome {
    let pieces = export::download(
        stretch.client.clone(),
        stretch.channel,
        stretch.date.clone(),
        stretch.start,
        LAST_SECOND_OF_DAY,
        false,
    );
    let mut reader = Reader::new();
    let started = Instant::now();
    // The first key frame tells the format of the video, which decides the pipeline.
    let first = loop {
        {
            let control = shared.control.lock().unwrap();
            if control.stop || control.revision != revision {
                return Outcome::Changed;
            }
        }
        match pieces.recv_timeout(CHECK_INTERVAL) {
            Ok(Ok(piece)) => {
                let frames = reader.push(&piece);
                if !frames.is_empty() {
                    break frames;
                }
            }
            Ok(Err(export::Error::Recorder(RecorderError::Unauthorized))) => {
                return Outcome::Refused;
            }
            Ok(Err(error)) => return lost(&error.to_string()),
            Err(RecvTimeoutError::Disconnected) => {
                let frames = reader.finish();
                if frames.is_empty() {
                    return lost("no recordings at this time");
                }
                break frames;
            }
            Err(RecvTimeoutError::Timeout) => {
                if started.elapsed() > STALL_TIMEOUT {
                    return lost("the recorder did not send the video");
                }
            }
        }
    };
    let progress = Progress::new();
    let codec = first[0].codec();
    let Some((pipeline, video)) =
        build_stretch_pipeline(shared.clone(), source, generation, progress.clone(), codec)
    else {
        return lost("could not build the pipeline");
    };
    let stop = Arc::new(AtomicBool::new(false));
    let feeder_stop = stop.clone();
    let feeder_pipeline = pipeline.clone();
    let rate = source.rate.max(1);
    std::thread::spawn(move || {
        feed(
            video,
            feeder_pipeline,
            reader,
            first,
            pieces,
            rate,
            feeder_stop,
        );
    });
    let outcome = watch(shared, source, &pipeline, revision, &progress, true);
    stop.store(true, Ordering::Relaxed);
    if progress.received_video.load(Ordering::Relaxed) {
        stretch.without_rtsp.lock().unwrap().insert(stretch.channel);
    }
    outcome
}

/// Passes frames to the pipeline until the stretch ends. It blocks while the pipeline has enough,
/// which is what holds the reading back.
fn feed(
    video: AppSrc,
    pipeline: gst::Pipeline,
    mut reader: Reader,
    first: Vec<Frame>,
    pieces: Receiver<Result<Vec<u8>, export::Error>>,
    rate: u32,
    stop: Arc<AtomicBool>,
) {
    let push = |frames: Vec<Frame>| {
        for frame in frames {
            if frame.key || rate < KEY_FRAMES_ONLY_RATE {
                let milliseconds = frame.elapsed_ms / u64::from(rate);
                let buffer = export::buffer_of(frame, milliseconds);
                // Fails once the pipeline stops, which also ends a push that was blocked.
                if video.push_buffer(buffer).is_err() {
                    return false;
                }
            }
        }
        true
    };
    if !push(first) {
        return;
    }
    while !stop.load(Ordering::Relaxed) {
        match pieces.recv_timeout(CHECK_INTERVAL) {
            Ok(Ok(piece)) => {
                if !push(reader.push(&piece)) {
                    return;
                }
            }
            Ok(Err(error)) => {
                fail(&pipeline, &error.to_string());
                return;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                if push(reader.finish()) {
                    let _ = video.end_of_stream();
                }
                return;
            }
        }
    }
}

fn decoder_elements(codec: Codec) -> (&'static str, &'static str) {
    match codec {
        Codec::H264 => ("h264parse", H264_DECODER),
        Codec::H265 => ("h265parse", H265_DECODER),
    }
}

fn build_stretch_pipeline(
    shared: Arc<Shared>,
    source: &Source,
    generation: u64,
    progress: Progress,
    codec: Codec,
) -> Option<(gst::Pipeline, AppSrc)> {
    let (parse, decoder) = decoder_elements(codec);
    let caps = match codec {
        Codec::H264 => "video/x-h264",
        Codec::H265 => "video/x-h265",
    };
    let description = format!(
        "appsrc name=video format=time block=true max-bytes=8388608 ! {parse} ! {decoder} \
         ! {DECODED_CAPS} ! appsink name=sink enable-last-sample=false sync=true max-buffers=1"
    );
    let pipeline = gst::parse::launch(&description)
        .ok()?
        .downcast::<gst::Pipeline>()
        .ok()?;
    share_decoder_device(&pipeline);
    let video = pipeline.by_name("video")?.downcast::<AppSrc>().ok()?;
    video.set_caps(Some(
        &gst::Caps::builder(caps)
            .field("stream-format", "byte-stream")
            .field("alignment", "au")
            .build(),
    ));
    let sink = pipeline.by_name("sink")?.downcast::<AppSink>().ok()?;
    let time_scale = f64::from(source.rate.max(1));
    attach_sink(&sink, shared, source, generation, progress, time_scale);
    Some((pipeline, video))
}

/// Takes the decoded frames to the item. `time_scale` turns the time of the stream into time of
/// the recording, for a stream with its timestamps sped up.
fn attach_sink(
    sink: &AppSink,
    shared: Arc<Shared>,
    source: &Source,
    generation: u64,
    progress: Progress,
    time_scale: f64,
) {
    let recording = source.recording;
    let attempt = source.attempt;
    // The decoder refuses DMABuf unless downstream declares VideoMeta, which appsink doesn't.
    sink.static_pad("sink")
        .unwrap()
        .add_probe(gst::PadProbeType::QUERY_DOWNSTREAM, |_, info| {
            if let Some(query) = info.query_mut() {
                if let gst::QueryViewMut::Allocation(allocation) = query.view_mut() {
                    allocation.add_allocation_meta::<VideoMeta>(None);
                }
            }
            gst::PadProbeReturn::Ok
        });
    let mut last_report = Instant::now();
    let mut format = FrameFormat::default();
    sink.set_callbacks(
        AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                if recording && last_report.elapsed() >= POSITION_INTERVAL {
                    if let Some(seconds) = stream_time_of(&sample) {
                        last_report = Instant::now();
                        shared.report_position(seconds * time_scale, attempt);
                    }
                }
                // A frame the item can't take doesn't count, so the stream ends up as lost.
                if push_sample(&shared, sample, generation, &mut format).is_some() {
                    progress.saw_frame();
                    if !progress.received_video.swap(true, Ordering::Relaxed) {
                        shared.set_status(true, false);
                        // The worker applies the speed and the shorter stall limit from now on.
                        shared.wake();
                    }
                }
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
}

/// rtspsrc answers every refusal with the same password, 8 times before it gives up. Past the few
/// requests a good login takes, the next one is refused here and it stops.
fn limit_logins(src: &gst::Element) {
    let requests = Arc::new(AtomicU32::new(0));
    let logged_in = Arc::new(AtomicBool::new(false));
    let seen = logged_in.clone();
    src.connect("before-send", false, move |_| {
        let sent = requests.fetch_add(1, Ordering::Relaxed);
        Some((seen.load(Ordering::Relaxed) || sent < MAX_REQUESTS_BEFORE_LOGIN).to_value())
    });
    src.connect("on-sdp", false, move |_| {
        logged_in.store(true, Ordering::Relaxed);
        None
    });
}

/// What the recorder answers when asked for a recording over RTSP.
#[derive(Debug, PartialEq)]
pub enum Access {
    /// The video arrived.
    Allowed,
    /// The user can't play this channel.
    Denied,
    /// Recordings of this channel answer 404 over RTSP, so asking tells nothing.
    NotOnRtsp,
    /// The password is wrong.
    Refused,
    /// Anything else, which the work that comes next deals with.
    Unknown,
}

/// Asks for a recording to learn the permission of a channel. Waits for the video, so a refusal at
/// any step of the request counts. Stops waiting once `cancel` is set.
pub fn recording_access(url: &str, user: &str, password: &str, cancel: &AtomicBool) -> Access {
    let Ok(src) = gst::ElementFactory::make("rtspsrc")
        .property("location", url)
        .property_from_str("protocols", "tcp")
        .property("user-id", user)
        .property("user-pw", password)
        .build()
    else {
        return Access::Unknown;
    };
    limit_logins(&src);
    let pipeline = gst::Pipeline::new();
    if pipeline.add(&src).is_err() {
        return Access::Unknown;
    }
    let playing = Arc::new(AtomicBool::new(false));
    let seen = playing.clone();
    let weak_pipeline = pipeline.downgrade();
    src.connect_pad_added(move |_, pad| {
        let Some(pipeline) = weak_pipeline.upgrade() else {
            return;
        };
        let Ok(sink) = gst::ElementFactory::make("fakesink")
            .property("sync", false)
            .property("signal-handoffs", true)
            .build()
        else {
            return;
        };
        let seen = seen.clone();
        sink.connect("handoff", false, move |_| {
            seen.store(true, Ordering::Relaxed);
            None
        });
        let _ = pipeline.add(&sink);
        let _ = sink.sync_state_with_parent();
        if let Some(input) = sink.static_pad("sink") {
            let _ = pad.link(&input);
        }
    });
    if pipeline.set_state(gst::State::Playing).is_err() {
        let _ = pipeline.set_state(gst::State::Null);
        return Access::Unknown;
    }
    let bus = pipeline.bus().expect("a pipeline has a bus");
    let started = Instant::now();
    let mut access = Access::Unknown;
    while started.elapsed() < PERMISSION_TIMEOUT
        && !playing.load(Ordering::Relaxed)
        && !cancel.load(Ordering::Relaxed)
    {
        let message = bus.timed_pop_filtered(
            gst::ClockTime::from_mseconds(100),
            &[gst::MessageType::Error],
        );
        if let Some(gst::MessageView::Error(error)) = message.as_ref().map(|m| m.view()) {
            if rtsp_status(error) == Some(RTSP_FORBIDDEN) {
                access = Access::Denied;
            } else if error.error().matches(gst::ResourceError::NotAuthorized) {
                access = Access::Refused;
            } else if error.error().matches(gst::ResourceError::NotFound) {
                access = Access::NotOnRtsp;
            }
            break;
        }
    }
    let _ = pipeline.set_state(gst::State::Null);
    if playing.load(Ordering::Relaxed) {
        access = Access::Allowed;
    }
    access
}

fn build_pipeline(
    shared: Arc<Shared>,
    source: &Source,
    generation: u64,
    progress: Progress,
) -> Option<gst::Pipeline> {
    let pipeline = gst::Pipeline::new();
    share_decoder_device(&pipeline);
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

    limit_logins(&src);

    src.connect("select-stream", false, |values| {
        let media = values[2].get::<gst::Caps>().ok().and_then(|caps| {
            let structure = caps.structure(0)?;
            structure.get::<String>("media").ok()
        });
        // The signal has to return a boolean, or GLib aborts.
        Some((media.as_deref() == Some("video")).to_value())
    });

    let source = source.clone();
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
        let (depay, (parse, decoder)) = match encoding.as_str() {
            "H264" => ("rtph264depay", decoder_elements(Codec::H264)),
            "H265" => ("rtph265depay", decoder_elements(Codec::H265)),
            other => {
                fail(&pipeline, &format!("unsupported encoding ({other})"));
                return;
            }
        };
        let description = format!(
            "{depay} ! {parse} name=parser ! {decoder} ! {DECODED_CAPS} \
             ! appsink name=sink enable-last-sample=false sync=false max-buffers=1 drop=true"
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
        // A frame that depends on a keyframe the decoder doesn't have comes out green, which is
        // what the stream starts with when it opens or changes speed between two keyframes.
        let waiting = progress.waiting_keyframe.clone();
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
        attach_sink(
            &sink,
            shared.clone(),
            &source,
            generation,
            progress.clone(),
            1.0,
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

/// Makes the decoders of a pipeline use the device of the interface, so frames don't leave the GPU.
#[cfg(windows)]
fn share_decoder_device(pipeline: &gst::Pipeline) {
    use gst::glib::translate::from_glib_full;
    let context = unsafe { rigel_decoder_context() };
    if context.is_null() {
        return;
    }
    let context: gst::Context = unsafe { from_glib_full(context) };
    pipeline.set_context(&context);
}

// One VA display shared by every pipeline saved about 200 MB with 31 cameras, but took about 10
// points more of a CPU core, so each pipeline keeps its own.
#[cfg(unix)]
fn share_decoder_device(_: &gst::Pipeline) {}

/// Reports a failure from a streaming thread to the loop in `watch`, which handles it as a lost stream.
fn fail(pipeline: &gst::Pipeline, reason: &str) {
    let _ = pipeline.post_message(gst::message::Error::new(gst::CoreError::Failed, reason));
}

#[cfg(unix)]
fn drm_format_of(caps: &gst::CapsRef) -> Option<(u32, u64)> {
    let drm = caps.structure(0)?.get::<String>("drm-format").ok()?;
    let (format, modifier) = drm.split_once(':').unwrap_or((drm.as_str(), "0x0"));
    let fourcc = match format {
        "NV12" => DRM_FORMAT_NV12,
        "P010" => DRM_FORMAT_P010,
        _ => return None,
    };
    let modifier = u64::from_str_radix(modifier.trim_start_matches("0x"), 16).ok()?;
    Some((fourcc, modifier))
}

/// Seconds of recording this frame is at, since the point the stream was opened at.
fn stream_time_of(sample: &gst::Sample) -> Option<f64> {
    let position = sample.buffer()?.pts()?;
    let segment = sample.segment()?.downcast_ref::<gst::ClockTime>()?;
    let time = segment.to_stream_time(position)?;
    Some(time.nseconds() as f64 / 1e9)
}

/// What a frame takes from the caps of its stream, read again only when the caps change, which
/// is rare next to the frames.
#[derive(Default)]
struct FrameFormat {
    caps: Option<gst::Caps>,
    #[cfg(windows)]
    width: i32,
    #[cfg(windows)]
    height: i32,
    #[cfg(unix)]
    fourcc: u32,
    #[cfg(unix)]
    modifier: u64,
    full_range: i32,
    matrix: i32,
}

impl FrameFormat {
    /// None when the frame can't be shown, as with a format the item doesn't draw.
    fn of(&mut self, sample: &gst::Sample) -> Option<&Self> {
        let caps = sample.caps()?;
        if self.caps.as_ref().map(|known| known.as_ptr()) != Some(caps.as_ptr()) {
            self.caps = None;
            // On Linux the size comes from the meta of each frame, and the caps of a DMABuf can't
            // be read as plain video.
            #[cfg(windows)]
            {
                let info = VideoInfo::from_caps(caps).ok()?;
                (self.width, self.height) = (info.width() as i32, info.height() as i32);
            }
            #[cfg(unix)]
            {
                (self.fourcc, self.modifier) = drm_format_of(caps)?;
            }
            (self.full_range, self.matrix) = color_of(caps).unwrap_or((0, 0));
            // A reference, since `to_owned` would copy the caps and never match the next frame.
            self.caps = sample.caps_owned();
        }
        Some(self)
    }
}

/// Whether the range is full, and the code of the color matrix for the video item.
fn color_of(caps: &gst::CapsRef) -> Option<(i32, i32)> {
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

#[cfg(windows)]
fn push_sample(
    shared: &Shared,
    sample: gst::Sample,
    generation: u64,
    format: &mut FrameFormat,
) -> Option<()> {
    let format = format.of(&sample)?;
    let mut frame = RigelFrame {
        width: format.width,
        height: format.height,
        fourcc: 0,
        modifier: 0,
        generation,
        n_planes: 0,
        full_range: format.full_range,
        matrix: format.matrix,
        fds: [-1; 4],
        offsets: [0; 4],
        strides: [0; 4],
        sample: std::ptr::null_mut(),
        owner: std::ptr::null_mut(),
        release: release_sample,
    };
    let item = shared.item.lock().unwrap();
    if *item == 0 {
        return None;
    }
    frame.owner = hand_over(sample);
    frame.sample = frame.owner;
    unsafe { rigel_item_set_frame(*item as *mut c_void, &frame) };
    Some(())
}

#[cfg(unix)]
fn push_sample(
    shared: &Shared,
    sample: gst::Sample,
    generation: u64,
    format: &mut FrameFormat,
) -> Option<()> {
    let format = format.of(&sample)?;
    let buffer = sample.buffer()?;
    let meta = buffer.meta::<VideoMeta>()?;
    let mut frame = RigelFrame {
        width: meta.width() as i32,
        height: meta.height() as i32,
        fourcc: format.fourcc,
        modifier: format.modifier,
        generation,
        n_planes: meta.n_planes() as i32,
        full_range: format.full_range,
        matrix: format.matrix,
        fds: [-1; 4],
        offsets: [0; 4],
        strides: [0; 4],
        sample: std::ptr::null_mut(),
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
    frame.owner = hand_over(sample);
    unsafe { rigel_item_set_frame(*item as *mut c_void, &frame) };
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// A recorder that answers every RTSP request with `status`.
    fn fake_recorder(status: &'static str) -> u16 {
        counting_recorder(status, "").0
    }

    /// The same, with `headers` in each answer, and a count of the requests it got.
    fn counting_recorder(status: &'static str, headers: &'static str) -> (u16, Arc<AtomicU32>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(AtomicU32::new(0));
        let counted = requests.clone();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut buffer = [0; 2048];
                while let Ok(size) = stream.read(&mut buffer) {
                    if size == 0 {
                        break;
                    }
                    counted.fetch_add(1, Ordering::Relaxed);
                    let request = String::from_utf8_lossy(&buffer[..size]);
                    let sequence = request
                        .lines()
                        .find(|line| line.to_lowercase().starts_with("cseq"))
                        .unwrap_or("CSeq: 1")
                        .to_owned();
                    let answer = format!("RTSP/1.0 {status}\r\n{sequence}\r\n{headers}\r\n");
                    let _ = stream.write_all(answer.as_bytes());
                }
            }
        });
        (port, requests)
    }

    #[test]
    fn tells_the_status_of_a_refused_stream() {
        gst::init().unwrap();
        let port = fake_recorder("403 Forbidden");
        let pipeline = gst::parse::launch(&format!(
            "rtspsrc location=rtsp://127.0.0.1:{port}/cam ! fakesink"
        ))
        .unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let bus = pipeline.bus().unwrap();
        let message = bus
            .timed_pop_filtered(gst::ClockTime::from_seconds(10), &[gst::MessageType::Error])
            .expect("an error from rtspsrc");
        let gst::MessageView::Error(error) = message.view() else {
            unreachable!();
        };
        assert_eq!(rtsp_status(error), Some(RTSP_FORBIDDEN));
        pipeline.set_state(gst::State::Null).unwrap();
    }

    // The limit leans on how rtspsrc retries, which no version of it promises to keep, and a wrong
    // password repeated can get a user locked out of the recorder.
    #[test]
    fn a_wrong_password_stops_after_a_few_requests() {
        gst::init().unwrap();
        let (port, requests) = counting_recorder(
            "401 Unauthorized",
            "WWW-Authenticate: Digest realm=\"recorder\", nonce=\"1\"\r\n",
        );
        let src = gst::ElementFactory::make("rtspsrc")
            .property("location", format!("rtsp://127.0.0.1:{port}/cam"))
            .property_from_str("protocols", "tcp")
            .property("user-id", "admin")
            .property("user-pw", "wrong")
            .build()
            .unwrap();
        limit_logins(&src);
        let pipeline = gst::Pipeline::new();
        pipeline.add(&src).unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let message = pipeline
            .bus()
            .unwrap()
            .timed_pop_filtered(gst::ClockTime::from_seconds(10), &[gst::MessageType::Error])
            .expect("an error from rtspsrc");
        let gst::MessageView::Error(error) = message.view() else {
            unreachable!();
        };
        pipeline.set_state(gst::State::Null).unwrap();
        assert!(error.error().matches(gst::ResourceError::NotAuthorized));
        assert!(requests.load(Ordering::Relaxed) <= MAX_REQUESTS_BEFORE_LOGIN);
    }

    #[cfg(unix)]
    #[test]
    fn eight_and_ten_bit_frames_are_taken() {
        gst::init().unwrap();
        let caps = |format: &str| {
            gst::Caps::builder("video/x-raw")
                .field("drm-format", format)
                .build()
        };
        assert_eq!(drm_format_of(&caps("NV12:0x0")), Some((DRM_FORMAT_NV12, 0)));
        assert_eq!(
            drm_format_of(&caps("P010:0x100000000000002")),
            Some((DRM_FORMAT_P010, 0x100000000000002))
        );
        assert_eq!(drm_format_of(&caps("YUY2")), None);
    }

    #[test]
    fn a_frame_format_reads_the_caps_once() {
        gst::init().unwrap();
        let caps = gst::Caps::builder("video/x-raw")
            .field("drm-format", "NV12:0x0")
            .field("colorimetry", "bt709")
            .field("format", "NV12")
            .field("width", 1920i32)
            .field("height", 1080i32)
            .build();
        let sample = |caps: &gst::Caps| gst::Sample::builder().caps(caps).build();
        let mut format = FrameFormat::default();
        let first = sample(&caps);
        assert_eq!(format.of(&first).map(|f| f.matrix), Some(1));
        let kept = format.caps.as_ref().map(|c| c.as_ptr());
        assert_eq!(kept, first.caps().map(|c| c.as_ptr()));
        format.of(&sample(&caps));
        assert_eq!(format.caps.as_ref().map(|c| c.as_ptr()), kept);
    }

    #[test]
    fn cancelling_stops_the_permission_check() {
        gst::init().unwrap();
        // Takes the connection and never answers, so only the cancel can end the wait.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let _held: Vec<_> = listener.incoming().flatten().collect();
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let later = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            later.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        let url = format!("rtsp://127.0.0.1:{port}/cam");
        assert_eq!(
            recording_access(&url, "user", "pw", &cancel),
            Access::Unknown
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn only_a_403_denies_recordings() {
        gst::init().unwrap();
        let url = |status| format!("rtsp://127.0.0.1:{}/cam", fake_recorder(status));
        let cancel = AtomicBool::new(false);
        let access = |status| recording_access(&url(status), "user", "pw", &cancel);
        assert_eq!(access("403 Forbidden"), Access::Denied);
        assert_eq!(access("404 Not Found"), Access::NotOnRtsp);
        assert_ne!(access("200 OK"), Access::Allowed);
        assert_eq!(access("401 Unauthorized"), Access::Refused);
    }
}
