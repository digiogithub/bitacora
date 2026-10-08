//! Embeds the application icon in `bitacora.exe` on Windows.
//!
//! GPUI loads icon resource 1 from the running module for the window and taskbar icon, and
//! Explorer shows the same resource for the file. macOS and Linux get the icon from the bundle
//! (`.icns`, `.desktop` + hicolor PNGs) produced by `cargo xtask bundle` and the Flatpak manifest.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../packaging/icons/bitacora.ico");
    #[cfg(windows)]
    windows_icon();
}

#[cfg(windows)]
fn windows_icon() {
    use std::path::PathBuf;

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default());
    let icon = manifest_dir.join("../../packaging/icons/bitacora.ico");
    // rc.exe string literals treat `\` as an escape; forward slashes are accepted in paths.
    let icon = icon.display().to_string().replace('\\', "/");
    let rc = out_dir.join("bitacora.rc");
    if let Err(err) = std::fs::write(&rc, format!("1 ICON \"{icon}\"\n")) {
        panic!("cannot write {}: {err}", rc.display());
    }
    if let Err(err) = embed_resource::compile(&rc, embed_resource::NONE).manifest_optional() {
        panic!("cannot embed the Windows icon: {err}");
    }
}
