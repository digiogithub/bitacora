---
created_at: 2026-10-08T08:50:09.711090537Z
updated_at: 2026-10-08T08:50:09.711090537Z
tags:
    - changes
    - packaging
    - icons
    - release
---
# App icon from the design system in every build

**What:**
- `packaging/icons/bitacora.svg` replaced by the design-system app icon (dark rounded square, book + ribbon mark, petrol ribbon) on a 1024 canvas with 64 px inset; new `packaging/icons/bitacora-small.svg` (favicon variant) for 16/32 px.
- `packaging/make-icons.sh`: small SVG for 16/32, ICO 256 entry stored as PNG (`icotool -r`, 370 KB → 106 KB). All PNG/ICO/ICNS regenerated.
- Windows: new `crates/bitacora-app/build.rs` embeds `bitacora.ico` as ICON resource 1 via `embed-resource` (`=3.0.12`, already in the lockfile through gpui-pre; workspace dep + `[target.'cfg(windows)'.build-dependencies]`). GPUI (`gpui-pre-windows` `load_icon`) loads resource 1 for window/taskbar; Explorer shows it for the exe.
- Linux: `APP_ID = "es.digio.bitacora"` in `crates/bitacora-app/src/views/title_bar.rs` (`main_window_options` sets `app_id`); `StartupWMClass=es.digio.bitacora` in `packaging/flatpak/es.digio.bitacora.desktop` and new deb/AppImage template `packaging/linux/bitacora.desktop` (`desktop-template` in `[package.metadata.packager.deb]`).
- macOS: unchanged path, `.icns` regenerated.
- `docs/design/release-process.md` documents where each platform gets the icon.

**Why:** User asked to ship the new logo in the binary and all installers (Flatpak, AppImage/deb, macOS, Windows).

**Verified:** `cargo clippy -p bitacora-app --all-targets -D warnings`, `cargo fmt --check`, `cargo deny check` OK; `cargo xtask bundle --no-build --formats deb` with a placeholder binary: .desktop has StartupWMClass, hicolor PNGs identical to the new ones. Full release link failed locally (missing `libxkbcommon-x11` dev lib on this machine). Windows resource embedding not exercised locally (build.rs path only runs on a Windows host) — CI windows job must confirm.

Related: [[website-github-pages]] [[release-process]]
