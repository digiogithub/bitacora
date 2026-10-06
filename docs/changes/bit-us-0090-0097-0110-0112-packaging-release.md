---
created_at: 2026-10-06T20:00:11.530784201Z
updated_at: 2026-10-06T20:00:11.530784201Z
tags:
    - change
    - packaging
    - release
---
# Packaging, release pipeline, CLI self-update, Flatpak (BIT-US-0090/0097/0110/0112)

Continues [[bitacora-full-development-plan]]; design in [[release-process]] (docs/design/release-process.md), background [[crate-stack]].

## What changed
- `crates/bitacora-app/Cargo.toml`: `[package.metadata.packager]` (cargo-packager 0.11.8), identifier es.digio.bitacora.
- `packaging/icons/*` (own SVG artwork, PNG/ICO/ICNS, `packaging/make-icons.sh`), `packaging/install/install.{sh,ps1}`, `packaging/flatpak/*` (manifest, desktop, metainfo).
- `xtask/src/bundle.rs` (`cargo xtask bundle`, env signing overlay), `xtask/src/release.rs` (`release-check`, `release-notes`, `sha256sums`, `bump`), wired in `xtask/src/main.rs`.
- `.github/workflows/bundle.yml`, `release.yml`, `flatpak.yml` (new; actionlint clean). Signing secrets optional.
- `crates/bitacora-cli/src/cmd/self_update.rs` + `self_update_tests.rs`, `SelfUpdate` subcommand in main.rs; new workspace deps ureq, tar, flate2, zip (cargo deny ok).

## Verification
- `cargo test -p bitacora-cli` (11 unit incl. 9 self-update with fake release server), `cargo test -p xtask` (19), clippy -D warnings on both, cargo deny check ok, actionlint ok.
- Local Linux `cargo xtask bundle --formats deb` produced a valid .deb. AppImage not buildable on this host (AppImageLauncher interference). flatpak-builder absent; cargo-sources generation and metainfo/desktop validation succeeded.
- Not verified (needs external resources): signing/notarization, macOS/Windows bundles, real release, Flathub, Flatpak sandbox behaviour.
