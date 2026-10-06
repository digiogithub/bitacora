---
id: BIT-T-0266
type: task
title: CommandQueue with single consumer thread, origins and reply handles
status: done
priority: critical
parent: BIT-US-0062
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, core]
estimate: 3
created: 2026-10-06T14:32:23Z
updated: 2026-10-06T18:39:25Z
started: 2026-10-06T18:28:39Z
closed: 2026-10-06T18:39:25Z
---

## Description
`crates/bitacora-core/src/queue.rs`: `CommandQueue::spawn(graph) -> QueueHandle`; `QueueHandle::submit(Command { origin: Origin::{Ui, Mcp, Sync, External}, cmd, label }) -> Reply<Result<TxSummary, CommandError>>` where `Reply` supports blocking `wait()` and a callback (so `bitacora-app` and the tokio-based MCP bridge can await without core depending on tokio). Uses `crossbeam-channel`. The consumer owns `Graph`, calls `Graph::commit`, then notifies subscribers (`GraphEvent`). Panics in a command are caught and reported, the queue keeps running.

## Acceptance Criteria
- Unit tests: ordering, reply delivery, panic isolation.
- No tokio dependency in `bitacora-core` (`cargo tree` check in CI).

## Notes
Story BIT-US-0062. Implements BIT-SP-0005.R1. ADR-012.
