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
        #[qproperty(QString, recorders)]
        #[qproperty(i32, selected)]
        #[qproperty(QString, channels)]
        #[qproperty(QString, error)]
        #[qproperty(QString, form_error, cxx_name = "formError")]
        #[qproperty(bool, form_busy, cxx_name = "formBusy")]
        type CameraGrid = super::CameraGridRust;

        /// Reads the saved recorders and opens the selected one.
        #[qinvokable]
        fn load(self: Pin<&mut Self>);

        /// Tests the connection and, if it works, saves the recorder and opens it.
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
        #[cxx_name = "selectRecorder"]
        fn select_recorder(self: Pin<&mut Self>, id: i32);

        #[qinvokable]
        #[cxx_name = "removeRecorder"]
        fn remove_recorder(self: Pin<&mut Self>, id: i32);

        /// Points an item at a channel. `visible` false stops its video, and `main_stream` picks
        /// the full resolution stream over the substream.
        #[qinvokable]
        unsafe fn show(
            self: Pin<&mut Self>,
            item: *mut QQuickItem,
            channel: i32,
            visible: bool,
            main_stream: bool,
        );
    }

    impl cxx_qt::Threading for CameraGrid {}
}

use core::pin::Pin;
use std::ffi::c_void;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use rigelcam_core::dahua::{Client, Error as RecorderError, Stream};
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
    recorders: QString,
    selected: i32,
    channels: QString,
    error: QString,
    form_error: QString,
    form_busy: bool,
    list: Option<RecorderList<SystemKeyring>>,
    connection: Option<Connection>,
}

impl qobject::CameraGrid {
    fn load(mut self: Pin<&mut Self>) {
        match RecorderList::open_default() {
            Ok(list) => self.as_mut().rust_mut().list = Some(list),
            Err(error) => {
                self.set_error(QString::from(error.to_string().as_str()));
                return;
            }
        }
        self.open_selected();
    }

    fn publish_recorders(mut self: Pin<&mut Self>) {
        let Some(list) = self.list.as_ref() else {
            return;
        };
        let items: Vec<_> = list
            .recorders()
            .iter()
            .map(|r| json!({"id": r.id, "name": r.name, "host": r.host}))
            .collect();
        let text = serde_json::Value::Array(items).to_string();
        let selected = list.selected().map_or(0, |r| r.id as i32);
        self.as_mut().set_recorders(QString::from(text.as_str()));
        self.as_mut().set_selected(selected);
    }

    /// Clears the grid and asks the selected recorder for its channels, off the interface thread.
    fn open_selected(mut self: Pin<&mut Self>) {
        self.as_mut().publish_recorders();
        self.as_mut().set_error(QString::default());
        self.as_mut().set_channels(QString::default());
        self.as_mut().rust_mut().connection = None;
        let Some(list) = self.list.as_ref() else {
            return;
        };
        let Some(recorder) = list.selected().cloned() else {
            return;
        };
        let password = match list.password(recorder.id) {
            Ok(password) => password,
            Err(error) => {
                self.set_error(QString::from(error.to_string().as_str()));
                return;
            }
        };
        let connection = Connection {
            host: recorder.host,
            user: recorder.user,
            password,
        };
        let selected = recorder.id as i32;
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = connection.client().channels();
            let _ = thread.queue(move |mut grid: Pin<&mut qobject::CameraGrid>| {
                // The user may have picked another recorder while this one was answering.
                if *grid.selected() != selected {
                    return;
                }
                match result {
                    Ok(channels) => {
                        grid.as_mut().rust_mut().connection = Some(connection);
                        let items: Vec<_> = channels
                            .iter()
                            .map(|c| json!({"number": c.number, "name": c.name}))
                            .collect();
                        let text = serde_json::Value::Array(items).to_string();
                        grid.as_mut().set_channels(QString::from(text.as_str()));
                    }
                    Err(error) => grid
                        .as_mut()
                        .set_error(QString::from(recorder_message(&error).as_str())),
                }
            });
        });
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
                    Ok(_) => grid.as_mut().open_selected(),
                    Err(error) => grid
                        .as_mut()
                        .set_form_error(QString::from(error.to_string().as_str())),
                }
                grid.as_mut().set_form_busy(false);
            });
        });
    }

    fn select_recorder(mut self: Pin<&mut Self>, id: i32) {
        let Some(result) = self
            .as_mut()
            .rust_mut()
            .list
            .as_mut()
            .map(|list| list.select(id as u64))
        else {
            return;
        };
        if let Err(error) = result {
            self.set_error(QString::from(error.to_string().as_str()));
            return;
        }
        self.open_selected();
    }

    fn remove_recorder(mut self: Pin<&mut Self>, id: i32) {
        let Some(result) = self
            .as_mut()
            .rust_mut()
            .list
            .as_mut()
            .map(|list| list.remove(id as u64))
        else {
            return;
        };
        if let Err(error) = result {
            self.set_error(QString::from(error.to_string().as_str()));
            return;
        }
        self.open_selected();
    }

    unsafe fn show(
        self: Pin<&mut Self>,
        item: *mut qobject::QQuickItem,
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
            .connection
            .as_ref()
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
