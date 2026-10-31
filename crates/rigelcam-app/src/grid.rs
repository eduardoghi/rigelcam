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
        #[qproperty(bool, sidebar_collapsed, cxx_name = "sidebarCollapsed")]
        #[qproperty(QString, theme)]
        #[qproperty(QString, language)]
        #[qproperty(QString, error)]
        #[qproperty(bool, list_corrupted, cxx_name = "listCorrupted")]
        #[qproperty(QString, notice)]
        #[qproperty(QString, missing)]
        #[qproperty(bool, video_unavailable, cxx_name = "videoUnavailable")]
        #[qproperty(QString, form_error, cxx_name = "formError")]
        #[qproperty(bool, form_busy, cxx_name = "formBusy")]
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
use std::collections::HashMap;
use std::ffi::c_void;
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use rigelcam_core::dahua::{Channel, Client, Error as RecorderError, Stream};
use rigelcam_core::layout::{CameraId, Layout};
use rigelcam_core::recorders::{
    Address, AddressError, Error as RecordersError, RecorderList, Secrets, SystemKeyring,
};
use serde_json::json;

use crate::camera_stream::{missing_elements, missing_h265_elements, CameraStream, Source};
use crate::i18n::{self, message};

extern "C" {
    fn rigel_item_set_stream(
        item: *mut c_void,
        stream: *mut c_void,
        destroy: extern "C" fn(*mut c_void),
    );
    fn rigel_item_stream(item: *mut c_void) -> *mut c_void;
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

const FIRST_RETRY_DELAY: Duration = Duration::from_secs(5);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);

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
}

impl Connection {
    fn client(&self) -> Client {
        client_for(&self.address, &self.user, &self.password)
    }
}

#[derive(Default)]
pub struct CameraGridRust {
    tree: QString,
    layout: QString,
    sidebar_collapsed: bool,
    theme: QString,
    language: QString,
    error: QString,
    list_corrupted: bool,
    notice: QString,
    missing: QString,
    video_unavailable: bool,
    form_error: QString,
    form_busy: bool,
    list: Option<RecorderList<SystemKeyring>>,
    connections: HashMap<u64, Connection>,
    channels: HashMap<u64, Vec<Channel>>,
    failures: HashMap<u64, String>,
    /// Latest try for each recorder, so an older answer or retry is ignored.
    attempts: HashMap<u64, u64>,
    retry_delays: HashMap<u64, Duration>,
}

impl qobject::CameraGrid {
    fn load(mut self: Pin<&mut Self>) {
        // The translator is already in place, from before QML started.
        let saved = RecorderList::open_default().ok();
        let language = saved.as_ref().and_then(|list| list.language());
        let code = i18n::resolve(language.unwrap_or_default());
        self.as_mut().set_language(QString::from(code));
        self.as_mut().check_components();
        match RecorderList::open_default() {
            Ok(list) => self.adopt_list(list),
            Err(RecordersError::Format(_)) => {
                self.as_mut().set_list_corrupted(true);
                self.set_error(QString::from(
                    "The recorder list is corrupted. The file was not changed.",
                ));
            }
            Err(error) => self.set_error(QString::from(error.to_string().as_str())),
        }
    }

    fn adopt_list(mut self: Pin<&mut Self>, list: RecorderList<SystemKeyring>) {
        let ids: Vec<u64> = list.recorders().iter().map(|r| r.id).collect();
        let layout = list.layout().cloned();
        let collapsed = list.sidebar_collapsed();
        let theme = QString::from(list.theme().unwrap_or_default());
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
            Ok(list) => {
                self.as_mut().set_list_corrupted(false);
                self.as_mut().set_error(QString::default());
                self.adopt_list(list);
            }
            Err(error) => self.set_error(QString::from(error.to_string().as_str())),
        }
    }

    // A missing element looks like an offline camera otherwise, on the first run of a new install.
    fn check_components(mut self: Pin<&mut Self>) {
        let missing = missing_elements();
        let h265 = missing_h265_elements();
        let message = if !missing.is_empty() {
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
        self.as_mut().set_video_unavailable(!missing.is_empty());
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
                    .map(|c| json!({"number": c.number, "name": c.name}))
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
                    let connection = Connection {
                        address: recorder.address(),
                        user: recorder.user,
                        password,
                    };
                    match connection.client().channels() {
                        Ok(channels) => Ok((connection, channels)),
                        Err(error) => {
                            let retry = matches!(error, RecorderError::Http(_));
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
        state
            .retry_delays
            .insert(id, (delay * 2).min(MAX_RETRY_DELAY));
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

    /// The grid follows the new layout even if saving fails, and the error goes to the toolbar.
    fn save_layout(mut self: Pin<&mut Self>, layout: Layout) {
        let result = self
            .as_mut()
            .rust_mut()
            .list
            .as_mut()
            .map(|list| list.set_layout(layout.clone()));
        let message = match result {
            Some(Err(error)) => error.to_string(),
            _ => String::new(),
        };
        self.as_mut().set_error(QString::from(message.as_str()));
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
        let result = self
            .as_mut()
            .rust_mut()
            .list
            .as_mut()
            .map(|list| list.set_language(code));
        if let Some(Err(error)) = result {
            self.as_mut()
                .set_error(QString::from(error.to_string().as_str()));
        }
        self.set_language(QString::from(code));
    }

    fn save_theme(mut self: Pin<&mut Self>, theme: &QString) {
        let text = theme.to_string();
        let result = self
            .as_mut()
            .rust_mut()
            .list
            .as_mut()
            .map(|list| list.set_theme(&text));
        if let Some(Err(error)) = result {
            self.as_mut()
                .set_error(QString::from(error.to_string().as_str()));
        }
        self.set_theme(theme.clone());
    }

    fn save_sidebar_collapsed(mut self: Pin<&mut Self>, collapsed: bool) {
        let result = self
            .as_mut()
            .rust_mut()
            .list
            .as_mut()
            .map(|list| list.set_sidebar_collapsed(collapsed));
        if let Some(Err(error)) = result {
            self.as_mut()
                .set_error(QString::from(error.to_string().as_str()));
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
                });
            let _ = thread.queue(move |mut grid: Pin<&mut qobject::CameraGrid>| {
                let model = match result {
                    Ok(model) => model,
                    Err(message) => {
                        grid.as_mut()
                            .set_form_error(QString::from(message.as_str()));
                        grid.as_mut().set_form_busy(false);
                        return;
                    }
                };
                let label = if name.trim().is_empty() { model } else { name };
                let new_password = Some(password.as_str()).filter(|p| !p.is_empty());
                let saved = grid.as_mut().rust_mut().list.as_mut().map(|list| match id {
                    Some(id) => list
                        .update(id, label.trim(), &address, &user, new_password)
                        .map(|()| id),
                    None => list.add(label.trim(), &address, &user, &password),
                });
                let Some(saved) = saved else {
                    return;
                };
                match saved {
                    Ok(saved_id) => {
                        if id.is_some() {
                            grid.as_mut().forget_recorder(saved_id);
                        }
                        grid.as_mut().connect_recorder(saved_id, id.is_none());
                    }
                    Err(error) => grid
                        .as_mut()
                        .set_form_error(QString::from(error.to_string().as_str())),
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
        // Every camera of the recorder reports it, and only the first one counts.
        if !self.connections.contains_key(&id) {
            return;
        }
        let message = message(REFUSED_MESSAGE, &[]);
        self.as_mut().forget_recorder(id);
        self.as_mut().rust_mut().failures.insert(id, message);
        self.publish_tree();
    }

    fn remove_recorder(mut self: Pin<&mut Self>, id: i32) {
        let id = id as u64;
        let Some(result) = self
            .as_mut()
            .rust_mut()
            .list
            .as_mut()
            .map(|list| list.remove(id))
        else {
            return;
        };
        let password_removed = match result {
            Ok(password_removed) => password_removed,
            Err(error) => {
                self.set_error(QString::from(error.to_string().as_str()));
                return;
            }
        };
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
            .filter(|_| visible)
            .map(|connection| {
                let kind = if main_stream {
                    Stream::Main
                } else {
                    Stream::Sub
                };
                Source {
                    url: connection.client().live_url(channel as u32, kind),
                    user: connection.user.clone(),
                    password: connection.password.clone(),
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
