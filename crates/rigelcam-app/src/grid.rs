//! Bridge between the QML grid and the cameras of a recorder.

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
        #[qproperty(QString, channels)]
        #[qproperty(QString, error)]
        type CameraGrid = super::CameraGridRust;

        /// Asks the recorder for its channels, without blocking the interface.
        #[qinvokable]
        fn load(self: Pin<&mut Self>);

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

use cxx_qt::Threading;
use cxx_qt_lib::QString;
use rigelcam_core::dahua::{Client, Stream};

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

#[derive(Clone)]
struct Settings {
    host: String,
    user: String,
    password: String,
}

impl Settings {
    // Temporary, until recorders can be added in the interface.
    fn from_env() -> Option<Self> {
        Some(Self {
            host: std::env::var("RIGELCAM_HOST").ok()?,
            user: std::env::var("RIGELCAM_USER").ok()?,
            password: std::env::var("RIGELCAM_PASSWORD").ok()?,
        })
    }

    fn client(&self) -> Client {
        Client::new(&self.host, &self.user, &self.password)
    }
}

#[derive(Default)]
pub struct CameraGridRust {
    channels: QString,
    error: QString,
}

fn json_escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| match c {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect(),
            c if (c as u32) < 0x20 => format!("\\u{:04x}", c as u32).chars().collect(),
            c => vec![c],
        })
        .collect()
}

impl qobject::CameraGrid {
    fn load(self: Pin<&mut Self>) {
        let Some(settings) = Settings::from_env() else {
            self.set_error(QString::from(
                "Defina RIGELCAM_HOST, RIGELCAM_USER e RIGELCAM_PASSWORD",
            ));
            return;
        };
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = settings.client().channels();
            let _ = thread.queue(
                move |mut grid: Pin<&mut qobject::CameraGrid>| match result {
                    Ok(channels) => {
                        let items: Vec<String> = channels
                            .iter()
                            .map(|c| {
                                format!(
                                    "{{\"number\":{},\"name\":\"{}\"}}",
                                    c.number,
                                    json_escape(&c.name)
                                )
                            })
                            .collect();
                        grid.as_mut()
                            .set_channels(QString::from(format!("[{}]", items.join(",")).as_str()));
                    }
                    Err(error) => grid
                        .as_mut()
                        .set_error(QString::from(error.to_string().as_str())),
                },
            );
        });
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
        let source = visible
            .then(|| Settings::from_env())
            .flatten()
            .map(|settings| {
                let kind = if main_stream {
                    Stream::Main
                } else {
                    Stream::Sub
                };
                Source {
                    url: settings.client().live_url(channel as u32, kind),
                    user: settings.user,
                    password: settings.password,
                }
            });
        (*stream).set_source(source);
    }
}
