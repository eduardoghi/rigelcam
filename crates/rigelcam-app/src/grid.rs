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
        #[qproperty(QString, error)]
        #[qproperty(QString, form_error, cxx_name = "formError")]
        #[qproperty(bool, form_busy, cxx_name = "formBusy")]
        type CameraGrid = super::CameraGridRust;

        /// Reads the saved recorders and connects to all of them.
        #[qinvokable]
        fn load(self: Pin<&mut Self>);

        /// Tests the connection and, if it works, saves the recorder and connects to it.
        #[qinvokable]
        #[cxx_name = "addRecorder"]
        fn add_recorder(
            self: Pin<&mut Self>,
            name: &QString,
            host: &QString,
            user: &QString,
            password: &QString,
        );

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

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use rigelcam_core::dahua::{Channel, Client, Error as RecorderError, Stream};
use rigelcam_core::layout::{CameraId, Layout};
use rigelcam_core::recorders::{RecorderList, SystemKeyring};
use serde_json::json;

use crate::camera_stream::{CameraStream, Source};

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

fn recorder_message(error: &RecorderError) -> String {
    match error {
        RecorderError::Http(_) => {
            "Não foi possível conectar ao gravador. Confira o endereço e a rede.".to_owned()
        }
        RecorderError::Unauthorized => "O gravador recusou o usuário ou a senha.".to_owned(),
        RecorderError::Status(code) => format!("O gravador respondeu com o erro HTTP {code}."),
        RecorderError::Challenge(_) | RecorderError::Unexpected(_) => {
            format!("O gravador enviou uma resposta inesperada ({error}).")
        }
    }
}

fn camera_id(recorder: i32, channel: i32) -> CameraId {
    CameraId {
        recorder: recorder as u64,
        channel: channel as u32,
    }
}

#[derive(Clone)]
struct Connection {
    host: String,
    user: String,
    password: String,
}

impl Connection {
    fn client(&self) -> Client {
        Client::new(&self.host, &self.user, &self.password)
    }
}

#[derive(Default)]
pub struct CameraGridRust {
    tree: QString,
    layout: QString,
    sidebar_collapsed: bool,
    error: QString,
    form_error: QString,
    form_busy: bool,
    list: Option<RecorderList<SystemKeyring>>,
    connections: HashMap<u64, Connection>,
    channels: HashMap<u64, Vec<Channel>>,
    failures: HashMap<u64, String>,
}

impl qobject::CameraGrid {
    fn load(mut self: Pin<&mut Self>) {
        let list = match RecorderList::open_default() {
            Ok(list) => list,
            Err(error) => {
                self.set_error(QString::from(error.to_string().as_str()));
                return;
            }
        };
        let ids: Vec<u64> = list.recorders().iter().map(|r| r.id).collect();
        let layout = list.layout().cloned();
        let collapsed = list.sidebar_collapsed();
        self.as_mut().rust_mut().list = Some(list);
        self.as_mut().set_sidebar_collapsed(collapsed);
        if let Some(layout) = layout {
            self.as_mut().publish_layout(&layout);
        }
        self.as_mut().publish_tree();
        for id in ids {
            self.as_mut().connect_recorder(id, false);
        }
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
                    "state": state,
                    "failure": failure,
                    "cameras": cameras,
                })
            })
            .collect();
        let text = serde_json::Value::Array(items).to_string();
        self.set_tree(QString::from(text.as_str()));
    }

    /// Asks a recorder for its channels, off the interface thread. With `fill_empty_grid`, an
    /// empty grid takes its cameras, which is what someone adding a recorder expects to see.
    fn connect_recorder(mut self: Pin<&mut Self>, id: u64, fill_empty_grid: bool) {
        let Some(list) = self.list.as_ref() else {
            return;
        };
        let Some(recorder) = list.recorders().iter().find(|r| r.id == id).cloned() else {
            return;
        };
        let password = match list.password(id) {
            Ok(password) => password,
            Err(error) => {
                let message = error.to_string();
                self.as_mut().rust_mut().failures.insert(id, message);
                self.publish_tree();
                return;
            }
        };
        let connection = Connection {
            host: recorder.host,
            user: recorder.user,
            password,
        };
        self.as_mut().rust_mut().failures.remove(&id);
        self.as_mut().publish_tree();
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = connection.client().channels();
            let _ = thread.queue(move |mut grid: Pin<&mut qobject::CameraGrid>| {
                // The recorder may have been removed while it was answering.
                let exists = grid
                    .list
                    .as_ref()
                    .is_some_and(|list| list.recorders().iter().any(|r| r.id == id));
                if !exists {
                    return;
                }
                match result {
                    Ok(channels) => {
                        let mut state = grid.as_mut().rust_mut();
                        state.connections.insert(id, connection);
                        state.channels.insert(id, channels.clone());
                        // Before the tree, so no camera outside the grid gets opened.
                        grid.as_mut().merge_layout(id, &channels, fill_empty_grid);
                    }
                    Err(error) => {
                        let message = recorder_message(&error);
                        grid.as_mut().rust_mut().failures.insert(id, message);
                    }
                }
                grid.publish_tree();
            });
        });
    }

    /// Fixes the saved grid with the channels of a recorder, or starts it with them the first time.
    fn merge_layout(self: Pin<&mut Self>, recorder: u64, channels: &[Channel], fill_empty: bool) {
        let numbers: Vec<u32> = channels.iter().map(|c| c.number).collect();
        let saved = self.list.as_ref().and_then(|list| list.layout().cloned());
        let mut layout = match saved.clone() {
            Some(layout) if !(fill_empty && layout.is_empty()) => layout,
            _ => Layout::fitting(&recorder_cameras(recorder, &numbers)),
        };
        layout.repair(recorder, &numbers);
        if saved.as_ref() != Some(&layout) {
            self.save_layout(layout);
        }
    }

    fn change_layout(self: Pin<&mut Self>, change: impl FnOnce(&mut Layout)) {
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
        if let Some(Err(error)) = result {
            self.as_mut()
                .set_error(QString::from(error.to_string().as_str()));
        }
        self.publish_layout(&layout);
    }

    fn publish_layout(self: Pin<&mut Self>, layout: &Layout) {
        let text = serde_json::to_string(layout).unwrap_or_default();
        self.set_layout(QString::from(text.as_str()));
    }

    fn open_recorder(self: Pin<&mut Self>, id: i32) {
        let id = id as u64;
        let Some(channels) = self.channels.get(&id) else {
            return;
        };
        let numbers: Vec<u32> = channels.iter().map(|c| c.number).collect();
        self.save_layout(Layout::fitting(&recorder_cameras(id, &numbers)));
    }

    fn add_camera(self: Pin<&mut Self>, recorder: i32, channel: i32) {
        self.change_layout(|layout| layout.add(camera_id(recorder, channel)));
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
        mut self: Pin<&mut Self>,
        name: &QString,
        host: &QString,
        user: &QString,
        password: &QString,
    ) {
        let name = name.to_string();
        let connection = Connection {
            host: host.to_string().trim().to_owned(),
            user: user.to_string(),
            password: password.to_string(),
        };
        self.as_mut().set_form_busy(true);
        self.as_mut().set_form_error(QString::default());
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = connection.client().device_type();
            let _ = thread.queue(move |mut grid: Pin<&mut qobject::CameraGrid>| {
                let model = match result {
                    Ok(model) => model,
                    Err(error) => {
                        grid.as_mut()
                            .set_form_error(QString::from(recorder_message(&error).as_str()));
                        grid.as_mut().set_form_busy(false);
                        return;
                    }
                };
                let label = if name.trim().is_empty() { model } else { name };
                let added = match grid.as_mut().rust_mut().list.as_mut() {
                    Some(list) => list.add(
                        label.trim(),
                        &connection.host,
                        &connection.user,
                        &connection.password,
                    ),
                    None => return,
                };
                match added {
                    Ok(id) => grid.as_mut().connect_recorder(id, true),
                    Err(error) => grid
                        .as_mut()
                        .set_form_error(QString::from(error.to_string().as_str())),
                }
                grid.as_mut().set_form_busy(false);
            });
        });
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
        if let Err(error) = result {
            self.set_error(QString::from(error.to_string().as_str()));
            return;
        }
        let mut state = self.as_mut().rust_mut();
        state.connections.remove(&id);
        state.channels.remove(&id);
        state.failures.remove(&id);
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
