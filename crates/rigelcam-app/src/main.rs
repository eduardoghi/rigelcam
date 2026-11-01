// A console would open beside the window on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod camera_stream;
mod export;
mod grid;
mod i18n;
#[cfg(windows)]
mod update;

use rigelcam_core::recorders::RecorderList;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

extern "C" {
    fn rigel_prepare_graphics();
    fn rigel_register_types();
    fn rigel_set_app_identity();
    fn rigel_has_window() -> bool;
}

fn main() {
    // Recorders sit on the local network, and rtspsrc would otherwise use the proxy of the system.
    // Scanning the plugins in a process of its own would flash a console on Windows.
    #[cfg(windows)]
    std::env::set_var("GST_REGISTRY_FORK", "no");
    if std::env::var_os("GIO_USE_PROXY_RESOLVER").is_none() {
        std::env::set_var("GIO_USE_PROXY_RESOLVER", "dummy");
    }
    gstreamer::init().expect("could not start GStreamer");
    #[cfg(windows)]
    std::env::set_var("QSG_RHI_BACKEND", "d3d11");
    #[cfg(not(windows))]
    std::env::set_var("QSG_RHI_BACKEND", "opengl");
    // Under X11 Qt defaults to GLX, and the video item imports frames through EGL.
    if std::env::var_os("QT_XCB_GL_INTEGRATION").is_none() {
        std::env::set_var("QT_XCB_GL_INTEGRATION", "xcb_egl");
    }
    // Always, since the interface customizes controls that other styles don't let it change.
    std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Basic");

    // Linux needs OpenGL ES for external textures, and Windows a device shared with the decoders.
    unsafe { rigel_prepare_graphics() };

    let mut app = QGuiApplication::new();
    unsafe {
        rigel_register_types();
        rigel_set_app_identity();
    }
    // Before QML loads, so the first screen already comes in the saved language.
    let saved = RecorderList::open_default().ok();
    let language = saved.as_ref().and_then(|list| list.language());
    i18n::install(i18n::resolve(language.unwrap_or_default()));
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/rigelcam/qml/Main.qml"));
    }
    // Without this the process would stay alive with no window to close.
    if !unsafe { rigel_has_window() } {
        eprintln!("the interface could not be loaded");
        std::process::exit(1);
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
