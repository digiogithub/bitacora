---
id: BIT-T-0034
type: task
title: Vendor a gpui_tokio-style bridge module
status: in_progress
priority: critical
parent: BIT-US-0024
milestone: BIT-M-0001
author: mcp
labels: [async, bitacora-app]
estimate: 2
created: 2026-10-06T14:27:34Z
updated: 2026-10-06T16:46:40Z
started: 2026-10-06T16:46:40Z
---

## Description
Create `crates/bitacora-app/src/tokio_bridge.rs` modelled on Zed's `crates/gpui_tokio/src/gpui_tokio.rs` (Apache-2.0, ~80 LOC): `pub fn init(cx: &mut App)` builds a `tokio::runtime::Builder::new_multi_thread().worker_threads(2..=4).enable_all()` runtime on its own thread(s) and stores a handle in a GPUI global; `Tokio::spawn(cx, fut) -> Task<Result<T>>` and `Tokio::spawn_result`, aborting the `JoinHandle` when the GPUI `Task` is dropped; shutdown with a timeout on app quit. Include the Apache-2.0 attribution header and a `THIRD_PARTY_NOTICES` entry (or equivalent in `crates/bitacora-app/NOTICE`). `tokio` is a dependency of `bitacora-app` only (and `bitacora-mcp`/`bitacora-cli`), never of `bitacora-core`.

## Acceptance Criteria
- `#[gpui::test]`: a spawned tokio future (using `tokio::time::sleep`) completes and returns its value to the GPUI task.
- `#[gpui::test]`: dropping the GPUI `Task` cancels the tokio task (observed via a drop guard / `AtomicBool`).
- License header and notice present.

## Notes
- [[gpui-and-gpui-kit]] §1.4; [[crate-stack]] §4.2. ADR-012, ADR-014.
