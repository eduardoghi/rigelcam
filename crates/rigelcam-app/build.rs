use cxx_qt_build::{CxxQtBuilder, QmlModule};

use std::path::Path;
use std::process::Command;

// English is in the sources, so only the other languages need a compiled translation.
fn compile_translation(language: &str) {
    let source = format!("i18n/rigelcam_{language}.ts");
    println!("cargo:rerun-if-changed={source}");
    let output = std::env::var("OUT_DIR").unwrap();
    let output = Path::new(&output).join(format!("rigelcam_{language}.qm"));
    let compiled = ["lrelease6", "lrelease", "/usr/lib/qt6/bin/lrelease"]
        .iter()
        .any(|tool| {
            Command::new(tool)
                .args(["-silent", &source, "-qm"])
                .arg(&output)
                .status()
                .is_ok_and(|status| status.success())
        });
    assert!(compiled, "lrelease from the Qt linguist tools is required");
}

fn main() {
    compile_translation("pt_BR");
    compile_translation("es");
    println!("cargo:rustc-link-lib=EGL");
    CxxQtBuilder::new_qml_module(QmlModule::new("rigelcam").qml_file("qml/Main.qml"))
        .qt_module("Quick")
        .qrc("resources.qrc")
        .files(["src/grid.rs"])
        .cpp_files(["cpp/video_item.h", "cpp/video_item.cpp"])
        .build();
}
