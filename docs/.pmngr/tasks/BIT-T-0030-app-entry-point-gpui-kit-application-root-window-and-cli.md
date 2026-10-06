---
id: BIT-T-0030
type: task
title: "App entry point: GPUI Kit application, Root window and CLI args"
status: in_progress
priority: critical
parent: BIT-US-0014
milestone: BIT-M-0001
author: mcp
labels: [ui, bitacora-app]
estimate: 3
created: 2026-10-06T14:27:12Z
updated: 2026-10-06T16:46:30Z
started: 2026-10-06T16:46:30Z
---

## Description
In `crates/bitacora-app/src/main.rs` + `src/app.rs`:
- Parse args with `clap` (`--graph <PATH>` optional, `--log-level`).
- Start the application via GPUI Kit 0.7's one-window entry point (which creates the `Root` overlay host needed by dialogs/notifications), with `gpui-kit = "=0.7.1"` only; import as `use gpui_kit::gpui::{...}`.
- Window: title "Bitacora — <graph name>" (or "Bitacora" without a graph), sensible default size, min size; a placeholder `Workspace` view rendering the graph path.
- `anyhow` error handling in `main`; a failure to open the window logs and exits non-zero.
- One `#[gpui::test]` that constructs the `Workspace` view in `TestAppContext`.
Check the exact entry-point API against docs.rs for gpui-kit 0.7.1 and the `hello_world` example.

## Acceptance Criteria
- `cargo run -p bitacora-app -- --graph fixtures/graphs/edge-cases` opens a window on the developer's OS.
- `cargo test -p bitacora-app` passes headless in CI.
- No direct `gpui` dependency (passes `cargo xtask check-deps`).

## Notes
- [[gpui-and-gpui-kit]] §1.1 (Application/Root), §1.6, §2.2 (Root), ADR-001.
