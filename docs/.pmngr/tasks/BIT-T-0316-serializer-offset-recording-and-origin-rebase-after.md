---
id: BIT-T-0316
type: task
title: Serializer offset recording and origin rebase after successful write
status: backlog
priority: critical
parent: BIT-US-0063
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, io]
estimate: 3
created: 2026-10-06T14:33:18Z
updated: 2026-10-06T14:33:18Z
---

## Description
Have the page serializer return `SerializedPage { bytes, spans: Vec<(BlockId, Range<usize>, LineLayout)> }`. On write success (reported back to the queue consumer), if the page's version is unchanged since serialization set `DiskSnapshot` (bytes, blake3, mtime, len), rebase every block's `Origin` from `spans`, clear `dirty`; if edits happened in between, rebase only the snapshot and keep the page dirty (origins of blocks unchanged since serialization are still rebased).

## Acceptance Criteria
- Test: after write all blocks are clean and `serialize()` equals the new disk bytes.
- Test: edit during in-flight write → page dirty, next write contains the edit.

## Notes
Story BIT-US-0063. Implements BIT-SP-0005.R7. Coordinates with the serializer from BIT-EP-0003.
