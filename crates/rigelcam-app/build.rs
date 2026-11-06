use cxx_qt_build::{CxxQtBuilder, QmlModule};

use std::path::Path;
use std::process::Command;

/// lrelease in the binaries of the Qt that cxx-qt builds with, which doesn't have to be on the PATH.
fn qt_tool() -> Option<String> {
    let qmake = std::env::var("QMAKE").unwrap_or_else(|_| "qmake6".to_owned());
    let output = Command::new(qmake)
        .args(["-query", "QT_INSTALL_BINS"])
        .output()
        .ok()?;
    let folder = String::from_utf8(output.stdout).ok()?;
    let tool = Path::new(folder.trim()).join("lrelease");
    Some(tool.to_string_lossy().into_owned())
}

// English is in the sources, so only the other languages need a compiled translation.
fn compile_translation(language: &str) {
    let source = format!("i18n/rigelcam_{language}.ts");
    println!("cargo:rerun-if-changed={source}");
    let output = std::env::var("OUT_DIR").unwrap();
    let output = Path::new(&output).join(format!("rigelcam_{language}.qm"));
    let mut tools = vec!["lrelease6".to_owned(), "lrelease".to_owned()];
    tools.extend(qt_tool());
    tools.push("/usr/lib/qt6/bin/lrelease".to_owned());
    let compiled = tools.iter().any(|tool| {
        Command::new(tool)
            .args(["-silent", &source, "-qm"])
            .arg(&output)
            .status()
            .is_ok_and(|status| status.success())
    });
    assert!(compiled, "lrelease from the Qt linguist tools is required");
}

/// Puts the icon inside the executable, which is what Windows shows for the file itself.
fn embed_icon() {
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let icon = Path::new(&root).join("../../assets/branding/rigelcam.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let output = std::env::var("OUT_DIR").unwrap();
    let script = Path::new(&output).join("icon.rc");
    let resource = Path::new(&output).join("icon.res");
    let path = icon.display().to_string().replace('\\', "/");
    std::fs::write(&script, format!("1 ICON \"{path}\"\n")).unwrap();
    let compiled = Command::new("rc.exe")
        .args(["/nologo", "/fo"])
        .arg(&resource)
        .arg(&script)
        .status()
        .is_ok_and(|status| status.success());
    assert!(compiled, "rc.exe from the Windows SDK is required");
    println!("cargo:rustc-link-arg-bins={}", resource.display());
}

fn main() {
    compile_translation("pt_BR");
    compile_translation("es");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    let include_paths = if windows {
        embed_icon();
        for library in ["d3d11", "dxgi", "d3dcompiler"] {
            println!("cargo:rustc-link-lib={library}");
        }
        pkg_config::probe_library("gstreamer-d3d11-1.0")
            .expect("the GStreamer D3D11 library is required")
            .include_paths
    } else {
        println!("cargo:rustc-link-lib=EGL");
        Vec::new()
    };
    let node = if windows {
        "cpp/video_node_d3d11.cpp"
    } else {
        "cpp/video_node_gles.cpp"
    };
    // qt-build-utils runs qmlcachegen before qmltyperegistrar, so the QML would be compiled against
    // the types of the previous build. A new property then crashes the app as it opens on Windows
    // until the QML changes again. Without the file every build works like a clean one.
    let types = "qt-build-utils/qml_modules/rigelcam/plugin.qmltypes";
    let _ = std::fs::remove_file(Path::new(&std::env::var("OUT_DIR").unwrap()).join(types));
    let builder = CxxQtBuilder::new_qml_module(
        QmlModule::new("rigelcam")
            .qml_file("qml/Main.qml")
            .qml_file("qml/playback.js"),
    )
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
