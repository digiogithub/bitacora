---
created_at: 2026-10-06T20:18:31.110789371Z
updated_at: 2026-10-06T20:18:31.110789371Z
tags:
    - change
    - app
    - runtime
    - release
---
# BIT-US-0086, BIT-US-0111, BIT-US-0100: single instance, crash reports, update checks

Links: [[bitacora-full-development-plan]], [[changes/bit-us-0078-0079-0080-palette-sidebars-runtime]], [[auto-update]], [[mcp-server]].

## What changed
- `crates/bitacora-runtime/src/instance.rs`: `Primary::acquire` takes an exclusive `File::try_lock` on `instance.lock`, publishes `instance.json` and listens on loopback TCP with a token; `forward`, `Launch`, abnormal-exit marker.
- `crates/bitacora-runtime/src/crash.rs`: `CrashReport`, `Redactor`, `GraphRoots`, `install_panic_hook`, `abnormal_exit_report`, `pending`, `dismiss`, `issue_url`.
- `crates/bitacora-cli`: `serve::acquire_instance` (refuses when an instance runs), crash hook in `main`.
- `crates/bitacora-app`: `instance.rs`, `crash.rs`, `update/{mod,release,service}.rs`, `app.rs` restructured (`open_workspace`, launch listener, `CurrentWorkspace` global, background mode), `ui::notify_action` and `ui::choose`, palette command CheckForUpdates, settings `keep_running_in_background` and `updates`, en.yml strings.
- `deny.toml`: ignore RUSTSEC-2024-0388 (derivative, via velopack). Workspace deps: velopack =1.2.161, semver.
- docs: ADR-025, `docs/design/auto-update.md`.

## Why
Single writer and MCP port ownership; local-only crash diagnostics; user-consented updates (ADR-018).

## Verification
cargo fmt, clippy -D warnings clean; `cargo test --workspace --locked --no-fail-fast`: 1198 passed, 0 failed; cargo deny and xtask check-deps OK.

## Left
Real Velopack update on a published release and the release-workflow packaging (BIT-T-0243) remain in_review.
