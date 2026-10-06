---
created_at: 2026-10-06T17:04:33.566066954Z
updated_at: 2026-10-06T17:04:33.566066954Z
tags:
    - change
    - ui
    - bitacora-app
---
# App window, tokio bridge and workspace shell (BIT-US-0014, BIT-US-0024, BIT-US-0025)

Continues [[bitacora-full-development-plan]]; see [[gpui-and-gpui-kit]] and [[crate-stack]].

## What changed (crates/bitacora-app, commit c7346bd)
- Entry point: `main.rs`, `app.rs` (`run`, `start`), `cli.rs` (`--graph`, `--log-level`, `--smoke-test`). Window opens through GPUI Kit's one-window `open_window` (Root). Title "Bitacora — <graph>".
- `ui/mod.rs` facade: the only place naming the kit crate; enforced by test `ui::tests::only_ui_names_gpui_kit`.
- `paths.rs` (`AppDirs`, `graph_data_dir` with stable FNV-1a hash), `logging.rs` (stderr + daily rolling file, 7 kept; RUST_LOG > --log-level > default).
- `tokio_bridge.rs` (original implementation of the gpui_tokio pattern; `NOTICE`), `events.rs` (`EventPump::attach`), `clippy.toml` disallowing `tokio::spawn` and `block_on`.
- Shell: `views/{workspace,sidebar,status_bar,panels}.rs`, `layout.rs` (workspace.json), `theme.rs` + `settings.rs`, `keymap.rs` + `assets/keymaps/default.json` (`secondary-` = cmd/ctrl), `assets/locales/en.yml` (rust-i18n v1 file format: plain nested keys, no `_version`).
- `.github/workflows/build-app.yml`: per-OS release artifacts + smoke test.

## Why
Proven entry point and shell for later UI stories; ADR-001, ADR-010, ADR-012, ADR-014.

## Verification
`cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -D warnings`, `cargo test --workspace --locked` (34 app tests), `cargo xtask check-deps`, `cargo deny check`, `cargo machete`: all clean. Window opened on host Wayland (Intel Vulkan) and under Xvfb/lavapipe (screenshot checked); `--smoke-test` exit 0, "first frame rendered in 575 ms".

## Open / manual
Per-OS window check from CI artifacts (Linux Wayland with output, macOS, Windows), live OS appearance change check (macOS, Linux DE), workflow not yet run on GitHub. Registry exposes only the default Light/Dark themes until extra theme JSON is loaded.
