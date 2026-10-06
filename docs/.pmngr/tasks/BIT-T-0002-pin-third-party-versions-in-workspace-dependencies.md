---
id: BIT-T-0002
type: task
title: Pin third-party versions in [workspace.dependencies]
status: backlog
priority: critical
parent: BIT-US-0001
milestone: BIT-M-0001
author: mcp
labels: [infra, workspace, dependencies]
estimate: 2
created: 2026-10-06T14:24:22Z
updated: 2026-10-06T15:16:22Z
---

## Description
Fill `[workspace.dependencies]` in the root `Cargo.toml` with the versions from [[crate-stack]] §4.1 / Recommendations, so member crates only write `foo.workspace = true`:
- UI: `gpui-kit = { version = "=0.7.1" }` (exact pin; never a direct `gpui` dependency).
- Async/HTTP/MCP: `tokio = "1.53"`, `axum = "0.8.9"`, `rmcp = { version = "~3.5", features = ["server", "macros", "schemars", "transport-streamable-http-server", "transport-streamable-http-server-session", "transport-io"] }`, `schemars = "1"`.
- Storage: `rusqlite = { version = "0.40", features = ["bundled", "functions", "hooks", "backup"] }`, `rusqlite_migration = "2.6"`.
- Parsing/diff: `pulldown-cmark = "0.13"`, `diffy = "0.5"`, `imara-diff = "0.2"`, `similar = "3.2"`, `edn-rs = "0.19"`.
- Git (ADR-007, ADR-020): `gix = "0.88"`, including the network client features (HTTPS with rustls, SSH transport, credentials) needed by the gix-only fallback backend; the system git CLI is invoked as a process only when installed (git is never bundled); `git2` NOT added now.
- Misc: `notify = "8.2"`, `notify-debouncer-full = "0.7"`, `uuid = { version = "1.27", features = ["v4", "v7", "serde"] }`, `jiff = "0.2"`, `nucleo = "0.5"`, `nucleo-matcher = "0.3"`, `directories = "6"`, `tracing = "0.1"`, `tracing-subscriber = { version = "0.3", features = ["env-filter"] }`, `tracing-appender`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `keyring = "4.2"`, `rust-i18n = "4.2"`, `thiserror = "2"`, `anyhow = "1"`, `clap = { version = "4", features = ["derive"] }`, `async-channel = "2.5"`, `fs-err`, `tempfile`, `ignore`, `walkdir`, `globset`, `regex`, `parking_lot`.
- Dev: `insta = "1.49"`, `proptest = "1.11"`, `criterion = "0.8"`.
Only declare here; crates opt in when they need a dependency (keeps `cargo machete` clean).

## Acceptance Criteria
- `cargo tree -i gpui-pre` shows exactly one `gpui-pre 0.3.8` (brought by `gpui-kit`).
- No member crate specifies a version inline for a dependency present in `[workspace.dependencies]`.
- `cargo build --workspace --locked` passes with the lockfile committed.

## Notes
- [[crate-stack]] §4.1 and Recommendations; [[gpui-and-gpui-kit]] §1.6; [[mcp-server]] (rmcp `~3.5`).
- ADR-001, ADR-007, ADR-010, ADR-020. The `jiff` vs `chrono` question stays open in [[crate-stack]]; `jiff` is the preferred default.
