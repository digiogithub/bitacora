---
id: BIT-T-0126
type: task
title: Resource subscriptions driven by writer/index change events
status: done
priority: medium
parent: BIT-US-0019
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, resources]
estimate: 3
created: 2026-10-06T14:29:55Z
updated: 2026-10-06T19:01:03Z
started: 2026-10-06T18:44:25Z
closed: 2026-10-06T19:01:03Z
---

## Description
`crates/bitacora-mcp/src/subscriptions.rs`: per-session subscription registry (URI set) fed by a `broadcast::Receiver<PageChanged{graph, page, block_uuids}>` from the core writer/index; on change, send `notifications/resources/updated` via the session's peer for matching page/block/journal URIs, coalescing bursts (250 ms). Clean up on session close.

## Acceptance Criteria
- Integration test: subscribe, write the page through the core facade, receive notification within 2 s over SSE; unsubscribed sessions receive nothing.

## Notes
Story BIT-US-0019. Implements BIT-SP-0007.R18.
