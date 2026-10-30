mod camera_stream;
mod grid;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

extern "C" {
    fn rigel_use_gles();
    fn rigel_register_types();
}

fn main() {
    gstreamer::init().expect("could not start GStreamer");
    std::env::set_var("QSG_RHI_BACKEND", "opengl");

    // Tiled NV12 can only be imported as an external texture, which needs OpenGL ES.
    unsafe { rigel_use_gles() };

    let mut app = QGuiApplication::new();
    unsafe { rigel_register_types() };
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/rigelcam/qml/Main.qml"));
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
