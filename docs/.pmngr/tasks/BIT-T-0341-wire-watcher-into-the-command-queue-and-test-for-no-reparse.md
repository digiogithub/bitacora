---
id: BIT-T-0341
type: task
title: Wire watcher into the command queue and test for no reparse loops
status: backlog
priority: high
parent: BIT-US-0067
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-watch, bitacora-app, test]
estimate: 2
created: 2026-10-06T14:34:13Z
updated: 2026-10-06T14:34:13Z
---

## Description
Start `GraphWatcher` on graph open in `bitacora-app` and `bitacora-cli serve`; translate `FileEvent` into `Command { origin: External, cmd: ExternalFileChanged { .. } }`; also forward to the index for files not loaded as pages. Integration test `crates/bitacora-core/tests/watch_echo.rs`: 100 consecutive edits through the queue → 0 reparses counted; external write via `std::fs` → page updated within 300 ms.

## Acceptance Criteria
- Reparse counter metric exposed for tests.
- Test is stable on Linux/macOS/Windows CI (generous timeouts, no sleeps-as-sync).

## Notes
Story BIT-US-0067. Implements BIT-SP-0005.R11, BIT-SP-0005.R12. Epic acceptance: "Our own writes never trigger a reparse loop".
