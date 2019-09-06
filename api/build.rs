use std::path::PathBuf;
use std::process::Command;
use std::{env, io};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_path = PathBuf::from(env::var("OUT_DIR")?);

    if !cfg!(any(target_os = "windows",)) {
        panic!("unsupported platform");
    }
    if cfg!(target_os = "windows") {
        println!("cargo:rustc-link-lib=dylib=ole32");
        println!("cargo:rustc-link-lib=dylib=user32");
        println!("cargo:rustc-link-lib=dylib=windowsapp");
    }

    if cfg!(target_os = "windows") {
        cc::Build::new()
            .flag("/EHsc")
            .flag("/std:c++17")
            .file("webview.cpp")
            .compile("webview");
    }

    bindgen::Builder::default()
        .header("webview.h")
        .generate()
        .map_err(|()| io::Error::new(io::ErrorKind::Other, "bindgen failed"))?
        .write_to_file(out_path.join("bindings.rs"))?;

    Ok(())
}
