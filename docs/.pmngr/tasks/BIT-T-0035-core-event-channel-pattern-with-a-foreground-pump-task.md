---
id: BIT-T-0035
type: task
title: Core event channel pattern with a foreground pump task
status: done
priority: high
parent: BIT-US-0024
milestone: BIT-M-0001
author: mcp
labels: [async, bitacora-app]
estimate: 2
created: 2026-10-06T14:27:34Z
updated: 2026-10-06T17:04:05Z
started: 2026-10-06T16:46:40Z
closed: 2026-10-06T17:04:05Z
---

## Description
Add `crates/bitacora-app/src/events.rs`: a generic `EventPump<E>` that takes an `async_channel::Receiver<E>` and an `Entity<V>` (held weakly) and, inside `cx.spawn`, applies each event via a closure then calls `cx.notify()`; it stops when the entity is dropped or the channel closes. Demonstrate with a `StatusBar` entity showing a heartbeat counter driven by a tokio interval sender (proves tokio -> channel -> GPUI path) and a `cx.background_spawn` producer (proves background -> UI). Add a clippy `disallowed-methods` entry for `tokio::runtime::Runtime::block_on` and `futures::executor::block_on` in the app crate.

## Acceptance Criteria
- `#[gpui::test]` sends N events and asserts the entity state after `run_until_parked()`.
- The pump task ends without panics when the entity is released.
- Clippy flags a `block_on` added to the app crate.

## Notes
- [[gpui-and-gpui-kit]] §1.4 (service -> UI notifications via async-channel); [[crate-stack]] §4.1 Channels row, §4.2, Risk R4.
