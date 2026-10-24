use std::{env, path::PathBuf, process::Command};
fn main() {
    let qt = PathBuf::from(
        env::var("QT_ROOT").expect("Set QT_ROOT to the Qt 6 MinGW SDK folder; run Build.ps1"),
    );
    let compiler = env::var("CXX").unwrap_or_else(|_| "g++".into());
    cc::Build::new()
        .cpp(true)
        .compiler(compiler)
        .flag("-std=c++17")
        .flag("-fPIC")
        .include(qt.join("include"))
        .include(qt.join("include/QtCore"))
        .include(qt.join("include/QtGui"))
        .include(qt.join("include/QtWidgets"))
        .file("src/qt_bridge.cpp")
        .compile("spacelens_qt");
    println!(
        "cargo:rustc-link-search=native={}",
        qt.join("lib").display()
    );
    for library in ["Qt6Widgets", "Qt6Gui", "Qt6Core"] {
        println!("cargo:rustc-link-lib={library}");
    }
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let resource = out.join("app.o");
    let status = Command::new("windres")
        .args(["assets/app.rc", "-O", "coff", "-o"])
        .arg(&resource)
        .status()
        .expect("MinGW windres is required");
    assert!(status.success(), "Could not compile application resources");
    println!(
        "cargo:rustc-link-arg-bin=SpaceLens_Rust={}",
        resource.display()
    );
    println!("cargo:rerun-if-changed=src/qt_bridge.cpp");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-env-changed=QT_ROOT");
}
