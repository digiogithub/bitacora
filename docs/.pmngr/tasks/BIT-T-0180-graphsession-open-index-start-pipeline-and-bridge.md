---
id: BIT-T-0180
type: task
title: "GraphSession: open index, start pipeline and bridge IndexEvents to GPUI"
status: done
priority: high
parent: BIT-US-0073
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui, index]
estimate: 3
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T18:44:23Z
started: 2026-10-06T18:28:35Z
closed: 2026-10-06T18:44:23Z
---

## Description
`crates/bitacora-app/src/graph_session.rs`: a GPUI `Entity<GraphSession>` that owns `Index`, `Indexer` and the watcher subscription for one graph. On open: `bitacora_index::open`, run reconcile on a background executor, request priority for today's journal, subscribe to `IndexEvent`s and re-emit them as GPUI events (`cx.emit`) on the foreground thread (use the bridge from BIT-US-0024). On switch/close: stop watcher, flush writer, drop connections.

## Acceptance Criteria
- `#[gpui::test]` with a temp fixture graph: views receive `FileReplaced` after a file is modified on disk.
- Closing a session leaves no threads running (join handles checked).
- `OpenOutcome::RebuiltAfterCorruption` produces a `Notification` toast "Index rebuilt".

## Notes
BIT-SP-0003.R1, BIT-SP-0003.R7. ADR-012.
