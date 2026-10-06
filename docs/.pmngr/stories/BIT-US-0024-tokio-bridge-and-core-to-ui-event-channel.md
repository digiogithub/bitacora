---
id: BIT-US-0024
type: story
title: Tokio bridge and core-to-UI event channel
status: backlog
priority: critical
parent: BIT-EP-0002
milestone: BIT-M-0001
author: mcp
labels: [ui, async, bitacora-app]
estimate: 3
created: 2026-10-06T14:27:20Z
updated: 2026-10-06T14:27:20Z
---

## Description
As a developer, I want a single, tested bridge between GPUI's executors and a dedicated tokio runtime, plus the pattern for streaming core events into views, so that the MCP server, network I/O and background work never hit "no reactor running" or block the main thread.

## Acceptance Criteria
- A tokio multi-thread runtime (2–4 workers) is created at app start, stored as a GPUI global, and shut down cleanly on quit.
- `Tokio::spawn(cx, fut)` returns a GPUI `Task` that aborts the tokio task when dropped (tested).
- An `async-channel` receiver polled by a `cx.spawn` foreground task updates an entity and calls `cx.notify()` (demonstrated with a timer-driven status bar counter and covered by `#[gpui::test]`).
- No `tokio::spawn` from GPUI tasks and no `block_on` on the main thread anywhere in the crate (grep check in CI or clippy `disallowed_methods`).

## Notes
- [[gpui-and-gpui-kit]] §1.4; [[crate-stack]] §4.2 (runtime diagram) and Risk R4.
- ADR-010, ADR-012. Vendored code from Zed's `gpui_tokio` is Apache-2.0 (allowed; keep attribution) — never copy from GPL Zed crates (ADR-014).
