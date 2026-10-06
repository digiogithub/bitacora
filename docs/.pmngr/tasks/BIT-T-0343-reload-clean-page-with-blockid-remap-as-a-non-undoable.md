---
id: BIT-T-0343
type: task
title: Reload clean page with BlockId remap as a non-undoable External change transaction
status: done
priority: high
parent: BIT-US-0068
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, io]
estimate: 3
created: 2026-10-06T14:34:14Z
updated: 2026-10-06T19:42:48Z
closed: 2026-10-06T19:42:48Z
---

## Description
`crates/bitacora-core/src/external/reload.rs`: for a clean page and new bytes, parse, `align`, then build the new block tree reusing old `BlockId`s for paired blocks and fresh ids for added ones; set `DiskSnapshot` to the new bytes; push a non-undoable `External change` marker into history (undo entries addressing removed ids will truncate). Emit `GraphEvent::Reloaded { page, remap }` so views remap selection/scroll/collapsed UI state.

## Acceptance Criteria
- Tests for BIT-SP-0005.R13 scenarios (selection survives; undo of earlier edit after unrelated external change works).
- Undo of an edit whose block was removed externally returns `HistoryTruncated`.

## Notes
Story BIT-US-0068. Implements BIT-SP-0005.R13, relates to BIT-SP-0004.R17.
