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
    let windows = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    let mut include_paths = Vec::new();
    if windows {
        let gstreamer = pkg_config::probe_library("gstreamer-d3d11-1.0")
            .expect("the GStreamer D3D11 library is required");
        include_paths = gstreamer.include_paths;
        for library in ["d3d11", "dxgi", "d3dcompiler"] {
            println!("cargo:rustc-link-lib={library}");
        }
    } else {
        println!("cargo:rustc-link-lib=EGL");
    }
    let node = if windows {
        "cpp/video_node_d3d11.cpp"
    } else {
        "cpp/video_node_gles.cpp"
    };
    let builder = CxxQtBuilder::new_qml_module(QmlModule::new("rigelcam").qml_file("qml/Main.qml"))
        .qt_module("Quick")
        .qrc("resources.qrc")
        .files(["src/grid.rs"])
        .cpp_files(["cpp/video_item.h", "cpp/video_item.cpp", node]);
    // Only adds the headers of the GStreamer library that the Windows node uses.
    let builder = unsafe {
        builder.cc_builder(|cc| {
            cc.includes(&include_paths);
        })
    };
    builder.build();
}
