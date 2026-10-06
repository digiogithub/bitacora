---
id: BIT-T-0315
type: task
title: WriteQueue scheduler with debounce, max delay and transaction batching
status: done
priority: critical
parent: BIT-US-0063
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, io]
estimate: 3
created: 2026-10-06T14:33:18Z
updated: 2026-10-06T18:52:16Z
closed: 2026-10-06T18:52:16Z
---

## Description
`crates/bitacora-core/src/writer/queue.rs`: `WriteQueue { pending: BTreeMap<PageKey, PendingWrite { first_dirty, last_change, batch: TxId }>, debounce: 400 ms, max_delay: 2 s }`; `due(now) -> Vec<Batch>` groups pages sharing a transaction batch; write jobs run on a dedicated I/O thread (not the UI thread); `RenameFile` executes before content writes in a batch. Injectable `Clock`. `flush_all_blocking()` for quit/close.

## Acceptance Criteria
- Fake-clock tests for the scenarios of BIT-SP-0005.R2.
- Pages of one transaction are written in the same batch.
- `flush_all_blocking` writes all pending pages.

## Notes
Story BIT-US-0063. Implements BIT-SP-0005.R2. [[block-editor]] §5.2–5.3.
