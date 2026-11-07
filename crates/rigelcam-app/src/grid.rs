//! Bridge between the QML interface and the saved recorders and their cameras.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "C++Qt" {
        include!(<QtQuick/QQuickItem>);
        #[qobject]
        type QQuickItem;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, tree)]
        #[qproperty(QString, layout)]
        #[qproperty(QString, recordings)]
        #[qproperty(QString, recorded_days, cxx_name = "recordedDays")]
        #[qproperty(bool, sidebar_collapsed, cxx_name = "sidebarCollapsed")]
        #[qproperty(QString, theme)]
        #[qproperty(QString, language)]
        #[qproperty(QString, app_version, cxx_name = "appVersion")]
        #[qproperty(QString, error)]
        #[qproperty(bool, list_corrupted, cxx_name = "listCorrupted")]
        #[qproperty(QString, notice)]
        #[qproperty(QString, missing)]
        #[qproperty(bool, video_unavailable, cxx_name = "videoUnavailable")]
        #[qproperty(QString, form_error, cxx_name = "formError")]
        #[qproperty(bool, form_busy, cxx_name = "formBusy")]
        #[qproperty(bool, ready)]
        #[qproperty(QString, exporting)]
        #[qproperty(bool, export_running, cxx_name = "exportRunning")]
        #[qproperty(QString, update_version, cxx_name = "updateVersion")]
        #[qproperty(f64, update_progress, cxx_name = "updateProgress")]
        #[qproperty(bool, update_running, cxx_name = "updateRunning")]
        #[qproperty(QString, update_error, cxx_name = "updateError")]
        #[qproperty(f64, export_progress, cxx_name = "exportProgress")]
        #[qproperty(i32, exports_waiting, cxx_name = "exportsWaiting")]
        type CameraGrid = super::CameraGridRust;

        /// Reads the saved recorders and connects to all of them.
        #[qinvokable]
        fn load(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "saveLanguage"]
        fn save_language(self: Pin<&mut Self>, language: &QString);

        #[qinvokable]
        #[cxx_name = "saveTheme"]
        fn save_theme(self: Pin<&mut Self>, theme: &QString);

        /// Sets a damaged recorder list aside and starts an empty one.
        #[qinvokable]
        #[cxx_name = "startOver"]
        fn start_over(self: Pin<&mut Self>);

        /// Tests the connection and, if it works, saves the recorder and connects to it.
        #[qinvokable]
        #[cxx_name = "addRecorder"]
        fn add_recorder(
            self: Pin<&mut Self>,
            name: &QString,
            host: &QString,
            http_port: i32,
            rtsp_port: i32,
            user: &QString,
            password: &QString,
        );

        /// Tests the connection and, if it works, saves the changes and reconnects.
        #[qinvokable]
        #[cxx_name = "updateRecorder"]
        fn update_recorder(
            self: Pin<&mut Self>,
            id: i32,
            name: &QString,
            host: &QString,
            http_port: i32,
            rtsp_port: i32,
            user: &QString,
            password: &QString,
        );

        /// Closes the cameras of a recorder after it refused the password, until someone acts.
        #[qinvokable]
        #[cxx_name = "recorderRefused"]
        fn recorder_refused(self: Pin<&mut Self>, id: i32);

        #[qinvokable]
        #[cxx_name = "removeRecorder"]
        fn remove_recorder(self: Pin<&mut Self>, id: i32);

        #[qinvokable]
        #[cxx_name = "retryRecorder"]
        fn retry_recorder(self: Pin<&mut Self>, id: i32);

        /// Replaces the grid with the cameras of a recorder, in the smallest size that fits them.
        #[qinvokable]
        #[cxx_name = "openRecorder"]
        fn open_recorder(self: Pin<&mut Self>, id: i32);

        #[qinvokable]
        #[cxx_name = "addCamera"]
        fn add_camera(self: Pin<&mut Self>, recorder: i32, channel: i32);

        #[qinvokable]
        #[cxx_name = "placeCamera"]
        fn place_camera(self: Pin<&mut Self>, recorder: i32, channel: i32, square: i32);

        #[qinvokable]
        #[cxx_name = "removeCamera"]
        fn remove_camera(self: Pin<&mut Self>, recorder: i32, channel: i32);

        #[qinvokable]
        #[cxx_name = "setGridColumns"]
        fn set_grid_columns(self: Pin<&mut Self>, columns: i32);

        #[qinvokable]
        #[cxx_name = "saveSidebarCollapsed"]
        fn save_sidebar_collapsed(self: Pin<&mut Self>, collapsed: bool);

        /// Asks for the recordings of a channel on a day, or on the current one of the recorder
        /// when `date` is empty. The answer goes to the `recordings` property.
        #[qinvokable]
        #[cxx_name = "searchRecordings"]
        fn search_recordings(self: Pin<&mut Self>, recorder: i32, channel: i32, date: &QString);

        /// Finds which days of a month, given as `YYYY-MM`, have recordings of a channel. The days
        /// reach the `recordedDays` property as they are found.
        #[qinvokable]
        #[cxx_name = "searchRecordedDays"]
        fn search_recorded_days(self: Pin<&mut Self>, recorder: i32, channel: i32, month: &QString);

        /// Saves a stretch of a recording as a video, after the ones already waiting. The camera
        /// exporting goes to the `exporting` property, and `exportFinished` tells how it ended.
        #[qinvokable]
        #[cxx_name = "exportClip"]
        fn export_clip(
            self: Pin<&mut Self>,
            recorder: i32,
            channel: i32,
            name: &QString,
            date: &QString,
            start: i32,
            end: i32,
        );

        /// Downloads and starts the version in `updateVersion`, then `updateStarted` asks the app
        /// to close, so the installer can replace its files.
        #[qinvokable]
        #[cxx_name = "installUpdate"]
        fn install_update(self: Pin<&mut Self>);

        #[qsignal]
        #[cxx_name = "updateStarted"]
        fn update_started(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "cancelExports"]
        fn cancel_exports(self: Pin<&mut Self>);

        /// `folder` and `file` are empty when it failed, and a cancelled export doesn't report.
        #[qsignal]
        #[cxx_name = "exportFinished"]
        fn export_finished(self: Pin<&mut Self>, folder: QString, file: QString, error: QString);

        /// Plays a recording in an item from `seconds` after the midnight of `date`. Returns the
        /// number of this attempt, which its signals carry, or 0 without a connection.
        #[qinvokable]
        unsafe fn play(
            self: Pin<&mut Self>,
            item: *mut QQuickItem,
            recorder: i32,
            channel: i32,
            date: &QString,
            seconds: i32,
            rate: i32,
        ) -> i32;

        #[qinvokable]
        #[cxx_name = "pausePlayback"]
        unsafe fn pause_playback(self: Pin<&mut Self>, item: *mut QQuickItem, paused: bool);

        /// Points an item at a camera. `visible` false stops its video, and `main_stream` picks
        /// the full resolution stream over the substream.
        #[qinvokable]
        unsafe fn show(
            self: Pin<&mut Self>,
            item: *mut QQuickItem,
            recorder: i32,
            channel: i32,
            visible: bool,
            main_stream: bool,
        );
    }

    impl cxx_qt::Threading for CameraGrid {}
}

use core::pin::Pin;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use rigelcam_core::clips;
use rigelcam_core::dahua::{
    days_in_month, is_clock, is_date, Channel, Client, Error as RecorderError, LoginGate, Stream,
};
use rigelcam_core::layout::{CameraId, Layout};
use rigelcam_core::recorders::{
    list_path, lock_instance, put_password, restore_password, take_password, Address, AddressError,
    Error as RecordersError, InstanceLock, Previous, RecorderList, Secrets, SystemKeyring,
};
use serde_json::json;

use crate::camera_stream::{
    missing_elements, missing_h265_elements, old_gstreamer, recording_access, Access, CameraStream,
    Direct, DirectCameras, Source, Stretch,
};
use crate::export::{self, Job};
use crate::i18n::{self, message};
#[cfg(windows)]
use crate::update;

extern "C" {
    fn rigel_item_set_stream(
        item: *mut c_void,
        stream: *mut c_void,
        destroy: extern "C" fn(*mut c_void),
    );
    fn rigel_item_stream(item: *mut c_void) -> *mut c_void;
    fn rigel_graphics_ready() -> bool;
}

extern "C" fn destroy_stream(stream: *mut c_void) {
    drop(unsafe { Box::from_raw(stream as *mut CameraStream) });
}

const REFUSED_MESSAGE: &str = "The recorder refused the user or password.";

fn recorder_message(error: &RecorderError) -> String {
    match error {
        RecorderError::Http(_) => message(
            "Could not connect to the recorder. Check the address and the network.",
            &[],
        ),
        RecorderError::Unauthorized => message(REFUSED_MESSAGE, &[]),
        RecorderError::Status(code) => message(
            "The recorder answered with HTTP error %1.",
            &[&code.to_string()],
        ),
        RecorderError::Challenge(_) | RecorderError::Unexpected(_) => {
            // The whole body of an answer can be in the error.
            let detail: String = error.to_string().chars().take(200).collect();
            message("The recorder sent an unexpected answer (%1).", &[&detail])
        }
    }
}

fn export_message(error: &export::Error) -> String {
    match error {
        export::Error::Recorder(error) => recorder_message(error),
        export::Error::Empty => message("There are no recordings in this stretch.", &[]),
        export::Error::Denied => message(
            "You don't have permission to see the recordings of this camera.",
            &[],
        ),
        export::Error::Stopped => message("The recorder stopped sending the video.", &[]),
        export::Error::Cancelled => String::new(),
        export::Error::Missing(element) => {
            message("The GStreamer element %1 is missing.", &[element])
        }
        export::Error::Write(detail) => message("Could not save the video (%1).", &[detail]),
    }
}

/// How long a channel found allowed spares the next exports of it from asking again. Short, since
/// the recorder doesn't stop a download it refused, so this check is what keeps the permission.
const PERMISSION_MEMORY: Duration = Duration::from_secs(10 * 60);

const FIRST_RETRY_DELAY: Duration = Duration::from_secs(5);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);

fn report_refused(thread: &CxxQtThread<qobject::CameraGrid>, recorder: i32) {
    let _ = thread.queue(move |grid: Pin<&mut qobject::CameraGrid>| {
        grid.recorder_refused(recorder);
    });
}

/// A second of the day as QML sends it, which a recording request takes as is.
fn second_of_day(value: i32) -> Option<u32> {
    (0..86_400).contains(&value).then_some(value as u32)
}

/// A channel as QML sends it. Recorders number them from 1.
fn channel_number(value: i32) -> Option<u32> {
    (value >= 1).then_some(value as u32)
}

fn camera_id(recorder: i32, channel: i32) -> CameraId {
    CameraId {
        recorder: recorder as u64,
        channel: channel as u32,
    }
}

fn address_message(error: AddressError) -> &'static str {
    match error {
        AddressError::Credentials => "Enter only the address, without user or password.",
        AddressError::Https => {
            "The app can't connect over HTTPS yet. Use the address without https://."
        }
        AddressError::Invalid => "Invalid address. Use an IP or a host name.",
    }
}

fn client_for(address: &Address, user: &str, password: &str) -> Client {
    Client::new(&address.host, user, password).with_ports(address.http_port, address.rtsp_port)
}

#[derive(Clone)]
struct Connection {
    address: Address,
    user: String,
    password: String,
    gate: Arc<LoginGate>,
    /// Channels whose recordings answer 404 over RTSP, so they stop asking.
    without_rtsp: Arc<Mutex<HashSet<u32>>>,
    /// When an export last found each channel allowed, so the exports right after it don't open
    /// another RTSP session, which the recorder logs as a login.
    allowed_recordings: Arc<Mutex<HashMap<u32, Instant>>>,
    direct_cameras: Arc<DirectCameras>,
    sessions_refused: Arc<Mutex<bool>>,
}

impl Connection {
    fn client(&self) -> Client {
        client_for(&self.address, &self.user, &self.password)
            .with_session_memory(self.sessions_refused.clone())
    }

    /// A client for work that isn't a stream, which also has to stop logging in after a refusal.
    /// It can wait for the recorder, so it stays off the interface thread.
    fn allowed_client(&self) -> Result<Client, RecorderError> {
        if self.gate.allows() {
            Ok(self.client())
        } else {
            Err(RecorderError::Unauthorized)
        }
    }

    fn refuse_after(&self, error: &RecorderError) {
        if matches!(error, RecorderError::Unauthorized) {
            self.gate.refuse();
        }
    }

    /// Whether an export of `channel` may download, asking `check` for its permission only when
    /// nothing remembered already answers it.
    fn check_recordings(
        &self,
        channel: u32,
        check: impl FnOnce() -> Access,
    ) -> Result<(), export::Error> {
        let recent = self
            .allowed_recordings
            .lock()
            .unwrap()
            .get(&channel)
            .is_some_and(|checked| checked.elapsed() < PERMISSION_MEMORY);
        // RTSP answers 404 for the recordings of these, so asking can't tell anything.
        if recent || self.without_rtsp.lock().unwrap().contains(&channel) {
            return Ok(());
        }
        match check() {
            Access::Denied => {
                self.allowed_recordings.lock().unwrap().remove(&channel);
                Err(export::Error::Denied)
            }
            // Downloading would send the same wrong password again.
            Access::Refused => Err(RecorderError::Unauthorized.into()),
            // Only what showed video is remembered, since a timeout or a cancel says nothing about
            // the permission.
            Access::Allowed => {
                let mut allowed = self.allowed_recordings.lock().unwrap();
                allowed.insert(channel, Instant::now());
                Ok(())
            }
            Access::NotOnRtsp => {
                self.without_rtsp.lock().unwrap().insert(channel);
                Ok(())
            }
            Access::Unknown => Ok(()),
        }
    }
}

/// The wait before the next try at a recorder that didn't answer, after waiting `delay`.
fn next_retry_delay(delay: Duration) -> Duration {
    (delay * 2).min(MAX_RETRY_DELAY)
}

#[derive(Default)]
pub struct CameraGridRust {
    tree: QString,
    layout: QString,
    recordings: QString,
    recorded_days: QString,
    /// Latest search of days, so an older one stops when another starts.
    days_search: Arc<AtomicU64>,
    /// Latest search of each channel, so an answer to an older one is ignored.
    searches: HashMap<(i32, i32), u64>,
    search: u64,
    playbacks: i32,
    sidebar_collapsed: bool,
    theme: QString,
    language: QString,
    app_version: QString,
    error: QString,
    list_corrupted: bool,
    notice: QString,
    missing: QString,
    video_unavailable: bool,
    form_error: QString,
    form_busy: bool,
    ready: bool,
    exporting: QString,
    export_running: bool,
    update_version: QString,
    update_progress: f64,
    update_running: bool,
    update_error: QString,
    #[cfg(windows)]
    update: Option<Arc<rigelcam_core::update::Manifest>>,
    export_progress: f64,
    exports_waiting: i32,
    exports: VecDeque<Job>,
    export_cancel: Arc<AtomicBool>,
    list: Option<RecorderList>,
    /// Held for as long as the app runs. Fields drop in order, so it comes after `list`, whose
    /// drop waits for the last save. Another instance can't read the file before that save ends.
    instance: Option<InstanceLock>,
    connections: HashMap<u64, Connection>,
    channels: HashMap<u64, Vec<Channel>>,
    failures: HashMap<u64, String>,
    /// Latest try for each recorder, so an older answer or retry is ignored.
    attempts: HashMap<u64, u64>,
    retry_delays: HashMap<u64, Duration>,
}

impl qobject::CameraGrid {
    fn load(mut self: Pin<&mut Self>) {
        self.as_mut()
            .set_app_version(QString::from(env!("CARGO_PKG_VERSION")));
        // The translator is already in place, from before QML started.
        let saved = RecorderList::open_default().ok();
        let language = saved.as_ref().and_then(|list| list.language());
        let code = i18n::resolve(language.unwrap_or_default());
        self.as_mut().set_language(QString::from(code));
        self.as_mut().check_components();
        match lock_instance() {
            Ok(Some(lock)) => {
                self.as_mut().rust_mut().instance = Some(lock);
                std::thread::spawn(|| clips::remove_unfinished(&clips::folder()));
            }
            Ok(None) => {
                let text = message("Rigelcam is already open.", &[]);
                self.set_error(QString::from(text.as_str()));
                return;
            }
            // A system that can't lock the file still gets to use the app.
            Err(_) => {}
        }
        match RecorderList::open_default() {
            Ok(list) => self.adopt_list(list),
            Err(RecordersError::Format(_)) => {
                self.as_mut().set_list_corrupted(true);
                let path = list_path().map_or(String::new(), |path| path.display().to_string());
                let text = message(
                    "The recorder list is corrupted. The file was not changed (%1).",
                    &[&path],
                );
                self.set_error(QString::from(text.as_str()));
            }
            Err(error) => self.set_error(QString::from(error.to_string().as_str())),
        }
    }

    #[cfg(windows)]
    fn check_for_update(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            update::remove_old_installers();
            let Some(manifest) = update::check() else {
                return;
            };
            let _ = thread.queue(move |mut grid: Pin<&mut qobject::CameraGrid>| {
                let version = QString::from(manifest.version.as_str());
                grid.as_mut().rust_mut().update = Some(Arc::new(manifest));
                grid.set_update_version(version);
            });
        });
    }

    #[cfg(not(windows))]
    fn check_for_update(self: Pin<&mut Self>) {}

    #[cfg(windows)]
    fn install_update(mut self: Pin<&mut Self>) {
        let Some(manifest) = self.update.clone().filter(|_| !self.update_running) else {
            return;
        };
        self.as_mut().set_update_error(QString::default());
        self.as_mut().set_update_progress(0.0);
        self.as_mut().set_update_running(true);
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let mut progress = |fraction: f64| {
                let _ = thread.queue(move |grid: Pin<&mut qobject::CameraGrid>| {
                    grid.set_update_progress(fraction);
                });
            };
            let result = update::install(&manifest, &mut progress);
            let _ = thread.queue(move |mut grid: Pin<&mut qobject::CameraGrid>| {
                grid.as_mut().set_update_running(false);
                match result {
                    Ok(()) => grid.update_started(),
                    Err(error) => {
                        let text = message("Could not update Rigelcam (%1).", &[&error]);
                        grid.set_update_error(QString::from(text.as_str()));
                    }
                }
            });
        });
    }

    /// No notice offers an update outside Windows, so this only closes the signal in the bridge.
    #[cfg(not(windows))]
    fn install_update(self: Pin<&mut Self>) {
        self.update_started();
    }

    fn adopt_list(mut self: Pin<&mut Self>, list: RecorderList) {
        self.as_mut().set_ready(true);
        if list.checks_for_updates() {
            self.as_mut().check_for_update();
        }
        let ids: Vec<u64> = list.recorders().iter().map(|r| r.id).collect();
        let layout = list.layout().cloned();
        let collapsed = list.sidebar_collapsed();
        let theme = QString::from(list.theme().unwrap_or_default());
        let thread = self.qt_thread();
        list.on_save_error(move |error| {
            let text = error.to_string();
            let _ = thread.queue(move |grid: Pin<&mut qobject::CameraGrid>| {
                grid.set_error(QString::from(text.as_str()));
            });
        });
        self.as_mut().rust_mut().list = Some(list);
        self.as_mut().set_sidebar_collapsed(collapsed);
        self.as_mut().set_theme(theme);
        if let Some(layout) = layout {
            self.as_mut().publish_layout(&layout);
        }
        self.as_mut().publish_tree();
        for id in ids {
            self.as_mut().connect_recorder(id, false);
        }
    }

    fn start_over(mut self: Pin<&mut Self>) {
        match RecorderList::start_over_default() {
            Ok((list, copy)) => {
                self.as_mut().set_list_corrupted(false);
                self.as_mut().set_error(QString::default());
                self.as_mut().adopt_list(list);
                let path = copy.display().to_string();
                let text = message("The damaged list was kept at %1.", &[&path]);
                self.set_notice(QString::from(text.as_str()));
            }
            Err(error) => self.set_error(QString::from(error.to_string().as_str())),
        }
    }

    // A missing element looks like an offline camera otherwise, on the first run of a new install.
    fn check_components(mut self: Pin<&mut Self>) {
        let missing = missing_elements();
        let h265 = missing_h265_elements();
        let old = old_gstreamer();
        let graphics_ready = unsafe { rigel_graphics_ready() };
        let message = if !graphics_ready {
            message("This graphics card could not be set up to show video.", &[])
        } else if let Some(version) = &old {
            message(
                "GStreamer 1.24 or newer is needed to show video, and this system has %1.",
                &[version],
            )
        } else if !missing.is_empty() && cfg!(windows) {
            // The plugins come with the app on Windows, so only a decoder the GPU lacks is missing.
            message(
                "This computer can't decode the video of the cameras on its graphics card (%1).",
                &[&missing.join(", ")],
            )
        } else if !missing.is_empty() {
            message(
                "GStreamer components are missing to show video (%1). \
                 Install the GStreamer plugins and VA-API support.",
                &[&missing.join(", ")],
            )
        } else if !h265.is_empty() {
            message(
                "H.265 cameras won't open because GStreamer components are missing (%1).",
                &[&h265.join(", ")],
            )
        } else {
            return;
        };
        self.as_mut()
            .set_video_unavailable(!graphics_ready || !missing.is_empty() || old.is_some());
        self.set_missing(QString::from(message.as_str()));
    }

    fn publish_tree(self: Pin<&mut Self>) {
        let Some(list) = self.list.as_ref() else {
            return;
        };
        let items: Vec<_> = list
            .recorders()
            .iter()
            .map(|r| {
                let channels = self.channels.get(&r.id);
                let failure = self.failures.get(&r.id);
                let state = match (channels, failure) {
                    (Some(_), _) => "online",
                    (None, Some(_)) => "failed",
                    (None, None) => "connecting",
                };
                let cameras: Vec<_> = channels
                    .into_iter()
                    .flatten()
                    .map(|c| {
                        let aspect = c
                            .main_size
                            .map_or(0.0, |(width, height)| f64::from(width) / f64::from(height));
                        json!({"number": c.number, "name": c.name, "aspect": aspect})
                    })
                    .collect();
                json!({
                    "id": r.id,
                    "name": r.name,
                    "host": r.host,
                    "user": r.user,
                    "httpPort": r.http_port,
                    "rtspPort": r.rtsp_port,
                    "state": state,
                    "failure": failure,
                    "cameras": cameras,
                })
            })
            .collect();
        let text = serde_json::Value::Array(items).to_string();
        self.set_tree(QString::from(text.as_str()));
    }

    /// Starts connecting to a recorder from scratch. With `fill_empty_grid`, an empty grid takes
    /// its cameras, which is what someone adding a recorder expects to see.
    fn connect_recorder(mut self: Pin<&mut Self>, id: u64, fill_empty_grid: bool) {
        let mut state = self.as_mut().rust_mut();
        state.failures.remove(&id);
        state.retry_delays.remove(&id);
        self.as_mut().publish_tree();
        self.attempt_recorder(id, fill_empty_grid);
    }

    /// Asks a recorder for its channels, off the interface thread. A network failure tries again
    /// by itself, because a recorder can take longer than the computer to come back from a power
    /// cut. A refused password doesn't, since repeating it can lock the user on the recorder.
    fn attempt_recorder(mut self: Pin<&mut Self>, id: u64, fill_empty_grid: bool) {
        let Some(list) = self.list.as_ref() else {
            return;
        };
        let Some(recorder) = list.recorders().iter().find(|r| r.id == id).cloned() else {
            return;
        };
        let attempt = {
            let mut state = self.as_mut().rust_mut();
            let attempt = state.attempts.entry(id).or_default();
            *attempt += 1;
            *attempt
        };
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            // Off the interface thread, since a locked keyring waits for the user to unlock it.
            let result = SystemKeyring
                .get(id)
                .map_err(|error| (error.to_string(), false))
                .and_then(|password| {
                    let address = recorder.address();
                    let gate = client_for(&address, &recorder.user, &password);
                    let connection = Connection {
                        address,
                        user: recorder.user,
                        password,
                        gate: Arc::new(LoginGate::new(gate)),
                        without_rtsp: Arc::default(),
                        allowed_recordings: Arc::default(),
                        direct_cameras: Arc::default(),
                        sessions_refused: Arc::default(),
                    };
                    match connection.client().channels() {
                        Ok(channels) => {
                            connection.gate.confirm();
                            Ok((connection, channels))
                        }
                        Err(error) => {
                            // A recorder that is starting answers errors for a while.
                            let retry = matches!(
                                error,
                                RecorderError::Http(_)
                                    | RecorderError::Status(500..=599)
                                    | RecorderError::Unexpected(_)
                            );
                            Err((recorder_message(&error), retry))
                        }
                    }
                });
            let _ = thread.queue(move |mut grid: Pin<&mut qobject::CameraGrid>| {
                // The recorder may have been removed or tried again while it was answering.
                if grid.attempts.get(&id) != Some(&attempt) {
                    return;
                }
                match result {
                    Ok((connection, channels)) => {
                        let mut state = grid.as_mut().rust_mut();
                        state.failures.remove(&id);
                        state.retry_delays.remove(&id);
                        state.connections.insert(id, connection);
                        state.channels.insert(id, channels.clone());
                        // Before the tree, so no camera outside the grid gets opened.
                        grid.as_mut().merge_layout(id, &channels, fill_empty_grid);
                    }
                    Err((message, retry)) => {
                        grid.as_mut().rust_mut().failures.insert(id, message);
                        if retry {
                            grid.as_mut().schedule_retry(id, attempt, fill_empty_grid);
                        }
                    }
                }
                grid.publish_tree();
            });
        });
    }

    fn schedule_retry(mut self: Pin<&mut Self>, id: u64, attempt: u64, fill_empty_grid: bool) {
        let mut state = self.as_mut().rust_mut();
        let delay = *state.retry_delays.get(&id).unwrap_or(&FIRST_RETRY_DELAY);
        state.retry_delays.insert(id, next_retry_delay(delay));
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let _ = thread.queue(move |grid: Pin<&mut qobject::CameraGrid>| {
                if grid.attempts.get(&id) == Some(&attempt) {
                    grid.attempt_recorder(id, fill_empty_grid);
                }
            });
        });
    }

    /// Starts the grid with the channels of a recorder the first time, and fixes the saved one.
    fn merge_layout(self: Pin<&mut Self>, recorder: u64, channels: &[Channel], fill_empty: bool) {
        let numbers: Vec<u32> = channels.iter().map(|c| c.number).collect();
        let saved = self.list.as_ref().and_then(|list| list.layout().cloned());
        let mut layout = match saved.clone() {
            Some(layout) if !(fill_empty && layout.is_empty()) => layout,
            _ => Layout::fitting(&recorder_cameras(recorder, &numbers)),
        };
        layout.repair();
        if saved.as_ref() != Some(&layout) {
            self.save_layout(layout);
        }
    }

    fn change_layout(mut self: Pin<&mut Self>, change: impl FnOnce(&mut Layout)) {
        self.as_mut().set_notice(QString::default());
        let layout = self.list.as_ref().and_then(|list| list.layout().cloned());
        let Some(mut layout) = layout else {
            return;
        };
        change(&mut layout);
        self.save_layout(layout);
    }

    /// The grid follows the new layout even if saving fails, and the error comes to the toolbar
    /// later, from the thread that saves.
    fn save_layout(mut self: Pin<&mut Self>, layout: Layout) {
        if let Some(list) = self.as_mut().rust_mut().list.as_mut() {
            list.set_layout(layout.clone());
        }
        self.as_mut().set_error(QString::default());
        self.publish_layout(&layout);
    }

    fn publish_layout(self: Pin<&mut Self>, layout: &Layout) {
        let text = serde_json::to_string(layout).unwrap_or_default();
        self.set_layout(QString::from(text.as_str()));
    }

    fn open_recorder(mut self: Pin<&mut Self>, id: i32) {
        let id = id as u64;
        let Some(channels) = self.channels.get(&id) else {
            return;
        };
        let numbers: Vec<u32> = channels.iter().map(|c| c.number).collect();
        let cameras = numbers.len();
        self.as_mut()
            .save_layout(Layout::fitting(&recorder_cameras(id, &numbers)));
        let capacity = Layout::capacity();
        let notice = if cameras > capacity {
            message(
                "Showing %1 of %2 cameras from this recorder.",
                &[&capacity.to_string(), &cameras.to_string()],
            )
        } else {
            String::new()
        };
        self.set_notice(QString::from(notice.as_str()));
    }

    fn search_recordings(mut self: Pin<&mut Self>, recorder: i32, channel: i32, date: &QString) {
        let search = self.search + 1;
        let mut state = self.as_mut().rust_mut();
        state.search = search;
        state.searches.insert((recorder, channel), search);
        // The screen waits for this answer, even when there is nothing to search with.
        let failure = move |error: String| {
            json!({"search": search, "recorder": recorder, "channel": channel, "error": error})
                .to_string()
        };
        let Some(connection) = self.connections.get(&(recorder as u64)).cloned() else {
            let text = failure(message("The recorder is not connected.", &[]));
            self.set_recordings(QString::from(text.as_str()));
            return;
        };
        let date = date.to_string();
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = connection.allowed_client().and_then(|client| {
                let now = client.clock()?;
                if !is_clock(&now) {
                    return Err(RecorderError::Unexpected(format!("clock {now}")));
                }
                if !date.is_empty() && !is_date(&date) {
                    return Err(RecorderError::Unexpected(format!("day {date}")));
                }
                let day = if date.is_empty() {
                    now.split(' ').next().unwrap_or_default().to_owned()
                } else {
                    date
                };
                let spans = client.recordings(channel as u32, &day)?;
                Ok((now, day, spans))
            });
            if let Err(error) = &result {
                connection.refuse_after(error);
            }
            let _ = thread.queue(move |mut grid: Pin<&mut qobject::CameraGrid>| {
                if grid.searches.get(&(recorder, channel)) != Some(&search) {
                    return;
                }
                match result {
                    Ok((now, day, spans)) => {
                        let spans: Vec<_> = spans
                            .iter()
                            .map(|s| json!({"start": s.start, "end": s.end, "event": s.event}))
                            .collect();
                        let text = json!({
                            "search": search,
                            "recorder": recorder,
                            "channel": channel,
                            "date": day,
                            "now": now,
                            "spans": spans,
                        })
                        .to_string();
                        grid.set_recordings(QString::from(text.as_str()));
                    }
                    Err(error) => {
                        let text = failure(recorder_message(&error));
                        grid.as_mut().set_recordings(QString::from(text.as_str()));
                        if matches!(error, RecorderError::Unauthorized) {
                            grid.recorder_refused(recorder);
                        }
                    }
                }
            });
        });
    }

    fn export_clip(
        mut self: Pin<&mut Self>,
        recorder: i32,
        channel: i32,
        name: &QString,
        date: &QString,
        start: i32,
        end: i32,
    ) {
        // Only days that came from a checked search get here, and this keeps it that way.
        if !is_date(&date.to_string()) {
            return;
        }
        // QML checks these too. A stretch that ends before it starts would ask the recorder for
        // nearly a whole day once the subtraction wraps.
        let (Some(channel), Some(start), Some(end)) = (
            channel_number(channel),
            second_of_day(start),
            second_of_day(end),
        ) else {
            return;
        };
        if end <= start {
            return;
        }
        let job = Job {
            recorder,
            channel,
            name: name.to_string(),
            date: date.to_string(),
            start,
            end,
        };
        self.as_mut().rust_mut().exports.push_back(job);
        self.start_next_export();
    }

    fn cancel_exports(mut self: Pin<&mut Self>) {
        let mut state = self.as_mut().rust_mut();
        state.exports.clear();
        state.export_cancel.store(true, Ordering::Relaxed);
        self.set_exports_waiting(0);
    }

    /// One at a time, since each download takes as much of the recorder as it can give.
    fn start_next_export(mut self: Pin<&mut Self>) {
        if self.export_running {
            let waiting = self.exports.len() as i32;
            self.set_exports_waiting(waiting);
            return;
        }
        let Some(job) = self.as_mut().rust_mut().exports.pop_front() else {
            self.set_exports_waiting(0);
            return;
        };
        let waiting = self.exports.len() as i32;
        self.as_mut().set_exports_waiting(waiting);
        let connection = self.connections.get(&(job.recorder as u64)).cloned();
        // A channel denied before is asked again by the check below, since the permission can
        // change on the recorder.
        let refusal = match &connection {
            None => Some("The recorder is not connected."),
            Some(_) => None,
        };
        let (Some(connection), None) = (connection, refusal) else {
            let error = message(refusal.unwrap_or_default(), &[]);
            let none = QString::default();
            self.as_mut()
                .export_finished(none.clone(), none, QString::from(error.as_str()));
            self.start_next_export();
            return;
        };
        // A cancel that came after the last export ended would stop this one before it starts.
        self.export_cancel.store(false, Ordering::Relaxed);
        self.as_mut().set_export_progress(0.0);
        self.as_mut()
            .set_exporting(QString::from(job.name.as_str()));
        self.as_mut().set_export_running(true);
        let cancel = self.export_cancel.clone();
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let mut progress = |fraction: f64| {
                let _ = thread.queue(move |grid: Pin<&mut qobject::CameraGrid>| {
                    grid.set_export_progress(fraction);
                });
            };
            // Checked between the steps before the download too, since closing the app waits for
            // a cancelled export. A login that is already waiting for the recorder can't be cut.
            let cancelled = || {
                if cancel.load(Ordering::Relaxed) {
                    Err(export::Error::Cancelled)
                } else {
                    Ok(())
                }
            };
            let result = cancelled()
                .and_then(|()| connection.allowed_client().map_err(export::Error::from))
                .and_then(|client| cancelled().map(|()| client))
                .and_then(|client| {
                    let url = client.playback_url(job.channel, &job.date, job.start);
                    connection.check_recordings(job.channel, || {
                        recording_access(&url, &connection.user, &connection.password, &cancel)
                    })?;
                    cancelled()?;
                    export::run(Arc::new(client), &job, &cancel, &mut progress)
                });
            if let Err(export::Error::Recorder(error)) = &result {
                connection.refuse_after(error);
            }
            let recorder = job.recorder;
            let _ = thread.queue(move |mut grid: Pin<&mut qobject::CameraGrid>| {
                grid.as_mut().set_exporting(QString::default());
                grid.as_mut().set_export_running(false);
                grid.as_mut()
                    .rust_mut()
                    .export_cancel
                    .store(false, Ordering::Relaxed);
                let none = QString::default;
                match &result {
                    Ok(path) => {
                        let part = |text: Option<&std::ffi::OsStr>| {
                            QString::from(text.unwrap_or_default().to_string_lossy().as_ref())
                        };
                        let folder = part(path.parent().map(|parent| parent.as_os_str()));
                        grid.as_mut()
                            .export_finished(folder, part(path.file_name()), none());
                    }
                    Err(export::Error::Cancelled) => {}
                    Err(error) => {
                        let text = export_message(error);
                        grid.as_mut()
                            .export_finished(none(), none(), QString::from(text.as_str()));
                        if matches!(error, export::Error::Recorder(RecorderError::Unauthorized)) {
                            grid.as_mut().recorder_refused(recorder);
                        }
                    }
                }
                grid.start_next_export();
            });
        });
    }

    fn search_recorded_days(self: Pin<&mut Self>, recorder: i32, channel: i32, month: &QString) {
        let Some(connection) = self.connections.get(&(recorder as u64)).cloned() else {
            return;
        };
        let month = month.to_string();
        if !is_date(&format!("{month}-01")) {
            return;
        }
        let latest = self.days_search.clone();
        let search = latest.fetch_add(1, Ordering::Relaxed) + 1;
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let refused = |error: RecorderError| {
                connection.refuse_after(&error);
                report_refused(&thread, recorder);
            };
            let client = match connection.allowed_client() {
                Ok(client) => client,
                Err(error) => return refused(error),
            };
            let today = match client.clock() {
                Ok(now) => now.split(' ').next().unwrap_or_default().to_owned(),
                Err(error @ RecorderError::Unauthorized) => return refused(error),
                Err(_) => String::new(),
            };
            // From the end, which is what someone looking for a recent event wants first.
            let dates: Vec<String> = (1..=days_in_month(&month))
                .rev()
                .map(|day| format!("{month}-{day:02}"))
                .filter(|date| today.is_empty() || *date <= today)
                .collect();
            let mut days: Vec<u32> = Vec::new();
            let key = format!("{recorder}:{channel}:{month}");
            let result = client.recorded_days(
                channel as u32,
                dates.iter().map(String::as_str),
                |date, recorded| {
                    if latest.load(Ordering::Relaxed) != search {
                        return false;
                    }
                    if recorded {
                        days.extend(date.rsplit('-').next().and_then(|d| d.parse::<u32>().ok()));
                        let text = json!({"key": key, "days": days, "done": false}).to_string();
                        let _ = thread.queue(move |grid: Pin<&mut qobject::CameraGrid>| {
                            grid.set_recorded_days(QString::from(text.as_str()));
                        });
                    }
                    true
                },
            );
            if let Err(error @ RecorderError::Unauthorized) = result {
                return refused(error);
            }
            if result.is_ok() && latest.load(Ordering::Relaxed) == search {
                let text = json!({"key": key, "days": days, "done": true}).to_string();
                let _ = thread.queue(move |grid: Pin<&mut qobject::CameraGrid>| {
                    grid.set_recorded_days(QString::from(text.as_str()));
                });
            }
        });
    }

    unsafe fn play(
        mut self: Pin<&mut Self>,
        item: *mut qobject::QQuickItem,
        recorder: i32,
        channel: i32,
        date: &QString,
        seconds: i32,
        rate: i32,
    ) -> i32 {
        if self.video_unavailable || !is_date(&date.to_string()) {
            return 0;
        }
        let (Some(channel), Some(seconds)) = (channel_number(channel), second_of_day(seconds))
        else {
            return 0;
        };
        if rate < 1 {
            return 0;
        }
        let attempt = self.playbacks + 1;
        self.as_mut().rust_mut().playbacks = attempt;
        let Some(connection) = self.connections.get(&(recorder as u64)) else {
            return 0;
        };
        let client = connection.client();
        let date = date.to_string();
        let url = client.playback_url(channel, &date, seconds);
        let source = Source {
            url,
            user: connection.user.clone(),
            password: connection.password.clone(),
            recording: true,
            rate: rate as u32,
            attempt,
            gate: connection.gate.clone(),
            stretch: Some(Arc::new(Stretch {
                client: Arc::new(client),
                channel,
                date,
                start: seconds,
                without_rtsp: connection.without_rtsp.clone(),
                allowed: connection.allowed_recordings.clone(),
            })),
            direct: None,
        };
        let item = item as *mut c_void;
        let mut stream = rigel_item_stream(item) as *mut CameraStream;
        if stream.is_null() {
            stream = Box::into_raw(Box::new(CameraStream::new(item)));
            rigel_item_set_stream(item, stream as *mut c_void, destroy_stream);
        }
        (*stream).set_paused(false);
        (*stream).set_source(Some(source));
        attempt
    }

    unsafe fn pause_playback(self: Pin<&mut Self>, item: *mut qobject::QQuickItem, paused: bool) {
        let stream = rigel_item_stream(item as *mut c_void) as *mut CameraStream;
        if !stream.is_null() {
            (*stream).set_paused(paused);
        }
    }

    fn add_camera(mut self: Pin<&mut Self>, recorder: i32, channel: i32) {
        let mut added = true;
        self.as_mut().change_layout(|layout| {
            added = layout.add(camera_id(recorder, channel));
        });
        if !added {
            self.set_notice(QString::from(
                "The grid is full. Remove a camera to add another.",
            ));
        }
    }

    fn place_camera(self: Pin<&mut Self>, recorder: i32, channel: i32, square: i32) {
        self.change_layout(|layout| layout.place(camera_id(recorder, channel), square as usize));
    }

    fn remove_camera(self: Pin<&mut Self>, recorder: i32, channel: i32) {
        self.change_layout(|layout| layout.remove(camera_id(recorder, channel)));
    }

    fn set_grid_columns(self: Pin<&mut Self>, columns: i32) {
        self.change_layout(|layout| layout.resize(columns as u32));
    }

    fn save_language(mut self: Pin<&mut Self>, language: &QString) {
        let code = i18n::resolve(&language.to_string());
        i18n::install(code);
        if let Some(list) = self.as_mut().rust_mut().list.as_mut() {
            list.set_language(code);
        }
        self.set_language(QString::from(code));
    }

    fn save_theme(mut self: Pin<&mut Self>, theme: &QString) {
        let text = theme.to_string();
        if let Some(list) = self.as_mut().rust_mut().list.as_mut() {
            list.set_theme(&text);
        }
        self.set_theme(theme.clone());
    }

    fn save_sidebar_collapsed(mut self: Pin<&mut Self>, collapsed: bool) {
        if let Some(list) = self.as_mut().rust_mut().list.as_mut() {
            list.set_sidebar_collapsed(collapsed);
        }
        self.set_sidebar_collapsed(collapsed);
    }

    fn add_recorder(
        self: Pin<&mut Self>,
        name: &QString,
        host: &QString,
        http_port: i32,
        rtsp_port: i32,
        user: &QString,
        password: &QString,
    ) {
        let ports = (http_port, rtsp_port);
        self.save_recorder(None, name, host, ports, user, password);
    }

    fn update_recorder(
        self: Pin<&mut Self>,
        id: i32,
        name: &QString,
        host: &QString,
        http_port: i32,
        rtsp_port: i32,
        user: &QString,
        password: &QString,
    ) {
        let ports = (http_port, rtsp_port);
        self.save_recorder(Some(id as u64), name, host, ports, user, password);
    }

    /// Tests the connection before saving. Editing with an empty password keeps the saved one.
    fn save_recorder(
        mut self: Pin<&mut Self>,
        id: Option<u64>,
        name: &QString,
        host: &QString,
        ports: (i32, i32),
        user: &QString,
        password: &QString,
    ) {
        self.as_mut().set_form_error(QString::default());
        self.as_mut().set_notice(QString::default());
        let port = |number: i32| u16::try_from(number).ok().filter(|&port| port != 0);
        let address = match Address::parse(&host.to_string(), port(ports.0), port(ports.1)) {
            Ok(address) => address,
            Err(error) => {
                self.set_form_error(QString::from(address_message(error)));
                return;
            }
        };
        let name = name.to_string();
        let user = user.to_string();
        let password = password.to_string();
        // The saved password is only for the address it was typed for, or changing the address
        // would hand it to whatever answers there.
        let same_address = |id| {
            self.list
                .as_ref()
                .and_then(|list| list.recorders().iter().find(|r| r.id == id))
                .is_some_and(|recorder| recorder.address() == address)
        };
        if id.is_some_and(|id| password.is_empty() && !same_address(id)) {
            self.set_form_error(QString::from("Enter the password for the new address."));
            return;
        }
        let Some(target) = (match id {
            Some(id) => Some(id),
            None => self
                .as_mut()
                .rust_mut()
                .list
                .as_mut()
                .map(|list| list.reserve_id()),
        }) else {
            return;
        };
        self.as_mut().set_form_busy(true);
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let saved_password = match id {
                Some(id) if password.is_empty() => SystemKeyring.get(id),
                _ => Ok(password.clone()),
            };
            let result = saved_password
                .map_err(|error| error.to_string())
                .and_then(|tested| {
                    client_for(&address, &user, &tested)
                        .device_type()
                        .map_err(|error| recorder_message(&error))
                })
                // Off the interface thread, since a locked keyring waits for the user to unlock it.
                .and_then(|model| {
                    if id.is_some() && password.is_empty() {
                        return Ok((model, None));
                    }
                    put_password(&SystemKeyring, target, &password)
                        .map(|previous| (model, Some(previous)))
                        .map_err(|error| error.to_string())
                });
            let _ = thread.queue(move |mut grid: Pin<&mut qobject::CameraGrid>| {
                let (model, previous) = match result {
                    Ok(saved) => saved,
                    Err(message) => {
                        grid.as_mut()
                            .set_form_error(QString::from(message.as_str()));
                        grid.as_mut().set_form_busy(false);
                        return;
                    }
                };
                let label = if name.trim().is_empty() { model } else { name };
                let saved = grid
                    .as_mut()
                    .rust_mut()
                    .list
                    .as_mut()
                    .map(|list| list.store(target, label.trim(), &address, &user));
                let Some(saved) = saved else {
                    return;
                };
                match saved {
                    Ok(()) => {
                        if id.is_some() {
                            grid.as_mut().forget_recorder(target);
                        }
                        grid.as_mut().connect_recorder(target, id.is_none());
                    }
                    Err(error) => {
                        // None when the password wasn't touched, which leaves nothing to undo.
                        if let Some(previous) = previous {
                            std::thread::spawn(move || {
                                restore_password(&SystemKeyring, target, &previous);
                            });
                        }
                        grid.as_mut()
                            .set_form_error(QString::from(error.to_string().as_str()));
                    }
                }
                grid.as_mut().set_form_busy(false);
            });
        });
    }

    /// Drops what was learned from a recorder, which closes its cameras once the tree is published.
    fn forget_recorder(self: Pin<&mut Self>, id: u64) {
        let mut state = self.rust_mut();
        state.connections.remove(&id);
        state.channels.remove(&id);
        state.failures.remove(&id);
        state.retry_delays.remove(&id);
        // Bumped and never reset, so a try still on its way can't pass for a later one.
        *state.attempts.entry(id).or_default() += 1;
    }

    fn recorder_refused(mut self: Pin<&mut Self>, id: i32) {
        let id = id as u64;
        // Every camera of the recorder reports it and only the first one counts. A report from
        // before the password was fixed finds a new gate, which nobody refused.
        let refused = self
            .connections
            .get(&id)
            .is_some_and(|connection| connection.gate.is_refused());
        if !refused {
            return;
        }
        let message = message(REFUSED_MESSAGE, &[]);
        self.as_mut().forget_recorder(id);
        self.as_mut().rust_mut().failures.insert(id, message);
        self.publish_tree();
    }

    fn remove_recorder(self: Pin<&mut Self>, id: i32) {
        let id = id as u64;
        let thread = self.qt_thread();
        // Off the interface thread, since a locked keyring waits for the user to unlock it.
        std::thread::spawn(move || {
            let (password, password_removed) = take_password(&SystemKeyring, id);
            let _ = thread.queue(move |grid: Pin<&mut qobject::CameraGrid>| {
                grid.finish_removing(id, password, password_removed);
            });
        });
    }

    fn finish_removing(
        mut self: Pin<&mut Self>,
        id: u64,
        password: Previous,
        password_removed: bool,
    ) {
        let Some(result) = self
            .as_mut()
            .rust_mut()
            .list
            .as_mut()
            .map(|list| list.forget(id))
        else {
            return;
        };
        if let Err(error) = result {
            if matches!(password, Previous::Password(_)) {
                std::thread::spawn(move || {
                    restore_password(&SystemKeyring, id, &password);
                });
            }
            self.set_error(QString::from(error.to_string().as_str()));
            return;
        }
        let notice = if password_removed {
            ""
        } else {
            "Recorder removed, but its password stayed in the system keyring."
        };
        self.as_mut().set_notice(QString::from(notice));
        self.as_mut().forget_recorder(id);
        let layout = self.list.as_ref().and_then(|list| list.layout().cloned());
        if let Some(layout) = layout {
            self.as_mut().publish_layout(&layout);
        }
        self.publish_tree();
    }

    fn retry_recorder(self: Pin<&mut Self>, id: i32) {
        self.connect_recorder(id as u64, false);
    }

    unsafe fn show(
        self: Pin<&mut Self>,
        item: *mut qobject::QQuickItem,
        recorder: i32,
        channel: i32,
        visible: bool,
        main_stream: bool,
    ) {
        let item = item as *mut c_void;
        let mut stream = rigel_item_stream(item) as *mut CameraStream;
        if stream.is_null() {
            // Each stream has a thread, which a camera outside the grid doesn't need.
            if !visible {
                return;
            }
            stream = Box::into_raw(Box::new(CameraStream::new(item)));
            rigel_item_set_stream(item, stream as *mut c_void, destroy_stream);
        }
        let source = self
            .connections
            .get(&(recorder as u64))
            .filter(|_| visible && !self.video_unavailable)
            .map(|connection| {
                let kind = if main_stream {
                    Stream::Main
                } else {
                    Stream::Sub
                };
                let camera = self
                    .channels
                    .get(&(recorder as u64))
                    .and_then(|channels| channels.iter().find(|c| c.number == channel as u32))
                    .and_then(|channel| channel.camera.as_ref());
                let client = Arc::new(connection.client());
                let direct = camera.map(|camera| {
                    Arc::new(Direct {
                        url: camera.live_url(kind),
                        channel: channel as u32,
                        cameras: connection.direct_cameras.clone(),
                        client: client.clone(),
                    })
                });
                Source {
                    url: client.live_url(channel as u32, kind),
                    user: connection.user.clone(),
                    password: connection.password.clone(),
                    recording: false,
                    rate: 1,
                    attempt: 0,
                    gate: connection.gate.clone(),
                    stretch: None,
                    direct,
                }
            });
        (*stream).set_source(source);
    }
}

fn recorder_cameras(recorder: u64, numbers: &[u32]) -> Vec<CameraId> {
    numbers
        .iter()
        .map(|&channel| CameraId { recorder, channel })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn connection() -> Connection {
        let address = Address::parse("127.0.0.1", None, None).unwrap();
        Connection {
            gate: Arc::new(LoginGate::new(client_for(&address, "u", "p"))),
            address,
            user: "u".to_owned(),
            password: "p".to_owned(),
            without_rtsp: Arc::default(),
            allowed_recordings: Arc::default(),
            direct_cameras: Arc::default(),
            sessions_refused: Arc::default(),
        }
    }

    /// Runs the check of channel 5 and tells whether it asked the recorder.
    fn check(connection: &Connection, answer: Access) -> (Result<(), export::Error>, bool) {
        let asked = Cell::new(false);
        let result = connection.check_recordings(5, || {
            asked.set(true);
            answer
        });
        (result, asked.get())
    }

    #[test]
    fn an_allowed_channel_is_not_asked_again_for_a_while() {
        let connection = connection();
        assert!(matches!(
            check(&connection, Access::Allowed),
            (Ok(()), true)
        ));
        assert!(matches!(
            check(&connection, Access::Denied),
            (Ok(()), false)
        ));
    }

    #[test]
    fn an_allowed_channel_is_asked_again_after_the_memory_runs_out() {
        let connection = connection();
        let long_ago = Instant::now() - PERMISSION_MEMORY - Duration::from_secs(1);
        connection
            .allowed_recordings
            .lock()
            .unwrap()
            .insert(5, long_ago);
        assert!(matches!(
            check(&connection, Access::Allowed),
            (Ok(()), true)
        ));
    }

    #[test]
    fn a_denied_channel_stops_the_export_and_is_asked_again_next_time() {
        let connection = connection();
        assert!(matches!(
            check(&connection, Access::Denied),
            (Err(export::Error::Denied), true)
        ));
        assert!(matches!(
            check(&connection, Access::Allowed),
            (Ok(()), true)
        ));
    }

    #[test]
    fn a_refused_password_stops_the_export_as_unauthorized() {
        let connection = connection();
        assert!(matches!(
            check(&connection, Access::Refused),
            (
                Err(export::Error::Recorder(RecorderError::Unauthorized)),
                true
            )
        ));
    }

    #[test]
    fn a_channel_without_rtsp_recordings_is_not_asked_again() {
        let connection = connection();
        assert!(matches!(
            check(&connection, Access::NotOnRtsp),
            (Ok(()), true)
        ));
        assert!(matches!(
            check(&connection, Access::Denied),
            (Ok(()), false)
        ));
    }

    #[test]
    fn an_unknown_answer_lets_the_export_go_and_is_asked_again() {
        let connection = connection();
        assert!(matches!(
            check(&connection, Access::Unknown),
            (Ok(()), true)
        ));
        assert!(matches!(
            check(&connection, Access::Unknown),
            (Ok(()), true)
        ));
    }

    #[test]
    fn values_from_qml_outside_a_day_or_a_channel_are_refused() {
        assert_eq!(second_of_day(0), Some(0));
        assert_eq!(second_of_day(86_399), Some(86_399));
        assert_eq!(second_of_day(86_400), None);
        assert_eq!(second_of_day(-1), None);
        assert_eq!(channel_number(1), Some(1));
        assert_eq!(channel_number(0), None);
        assert_eq!(channel_number(-5), None);
    }

    #[test]
    fn retries_wait_twice_as_long_up_to_a_minute() {
        let mut delay = FIRST_RETRY_DELAY;
        let mut waits = Vec::new();
        for _ in 0..6 {
            waits.push(delay.as_secs());
            delay = next_retry_delay(delay);
        }
        assert_eq!(waits, [5, 10, 20, 40, 60, 60]);
    }

    #[test]
    fn an_unexpected_answer_keeps_only_the_start_of_its_body() {
        let body = "x".repeat(1000);
        let text = recorder_message(&RecorderError::Unexpected(body));
        assert!(text.starts_with("The recorder sent an unexpected answer (%1)."));
        assert!(text.chars().count() < 300);
    }

    #[test]
    fn a_cancelled_export_shows_no_message() {
        assert!(export_message(&export::Error::Cancelled).is_empty());
    }
}
