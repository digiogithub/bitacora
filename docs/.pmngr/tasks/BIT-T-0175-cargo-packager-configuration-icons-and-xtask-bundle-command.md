---
id: BIT-T-0175
type: task
title: cargo-packager configuration, icons and xtask bundle command
status: backlog
priority: high
parent: BIT-US-0090
milestone: BIT-M-0005
author: mcp
labels: [packaging, bitacora-app, xtask]
estimate: 3
created: 2026-10-06T14:30:54Z
updated: 2026-10-06T14:30:54Z
---

## Description
- Add `[package.metadata.packager]` to `crates/bitacora-app/Cargo.toml` (cargo-packager 0.11.x): `product-name = "Bitacora"`, `identifier = "es.digio.bitacora"`, `binaries`, `icons` (from `assets/icons/` — 32/128/256/512 PNG, `.icns`, `.ico`), `category = "Productivity"`, `formats` per OS, `licenseFile = "LICENSE"`, deep-link/file association placeholders, `before-packaging-command = "cargo build -p bitacora-app --release"`.
- `cargo xtask bundle [--formats ...] [--sign]` wrapping `cargo packager --release` and collecting outputs into `target/dist/` with names `bitacora-<version>-<os>-<arch>.<ext>`.
- Linux: `.desktop` entry (StartupWMClass matching the GPUI app id), AppStream metainfo XML; deb `depends` on `libvulkan1`, `libxkbcommon0`, `libwayland-client0`, `libfontconfig1`.

## Acceptance Criteria
- `cargo xtask bundle` produces unsigned dmg (macOS), msi + nsis (Windows), deb + AppImage (Linux) locally.
- Installing the deb on a clean Ubuntu 24.04 VM pulls the right deps and launches the app.

## Notes
- [[crate-stack]] §5.1 (`xtask/`: bundle), §5.2; [[gpui-and-gpui-kit]] §1.8.
