mod camera_stream;
mod grid;
mod i18n;

use rigelcam_core::recorders::RecorderList;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

extern "C" {
    fn rigel_use_gles();
    fn rigel_register_types();
    fn rigel_set_app_identity();
}

fn main() {
    gstreamer::init().expect("could not start GStreamer");
    std::env::set_var("QSG_RHI_BACKEND", "opengl");
    // Under X11 Qt defaults to GLX, and the video item imports frames through EGL.
    if std::env::var_os("QT_XCB_GL_INTEGRATION").is_none() {
        std::env::set_var("QT_XCB_GL_INTEGRATION", "xcb_egl");
    }
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Basic");
    }

    // Tiled NV12 can only be imported as an external texture, which needs OpenGL ES.
    unsafe { rigel_use_gles() };

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
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
