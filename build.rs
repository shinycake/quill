//! Windows only: embed `assets/icons/Quill.ico` as the application icon of
//! `quill.exe` (a no-op on every other host).

#[cfg(windows)]
fn main() {
    use std::path::PathBuf;
    println!("cargo:rerun-if-changed=assets/icons/Quill.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let icon = manifest.join("assets/icons/Quill.ico");
    let rc = PathBuf::from(std::env::var("OUT_DIR").expect("out dir")).join("quill.rc");
    let icon = icon.to_string_lossy().replace('\\', "/");
    std::fs::write(&rc, format!("1 ICON \"{icon}\"\n")).expect("write rc");
    embed_resource::compile(&rc, embed_resource::NONE)
        .manifest_required()
        .expect("embed application icon");
}

#[cfg(not(windows))]
fn main() {}
