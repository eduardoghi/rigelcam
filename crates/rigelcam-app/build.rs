use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    println!("cargo:rustc-link-lib=EGL");
    CxxQtBuilder::new_qml_module(QmlModule::new("rigelcam").qml_file("qml/Main.qml"))
        .qt_module("Quick")
        .files(["src/grid.rs"])
        .cpp_files(["cpp/video_item.h", "cpp/video_item.cpp"])
        .build();
}
