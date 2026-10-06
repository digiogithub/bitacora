---
id: BIT-T-0342
type: task
title: "Block alignment algorithm: uuid match then (parent path, text) LCS"
status: done
priority: high
parent: BIT-US-0068
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, merge]
estimate: 3
created: 2026-10-06T14:34:13Z
updated: 2026-10-06T19:42:48Z
closed: 2026-10-06T19:42:48Z
---

## Description
`crates/bitacora-core/src/external/align.rs`: `align(old: &[FlatBlock], new: &[FlatBlock]) -> Alignment { pairs: Vec<(old_idx, new_idx)>, removed, added }`. Pass 1: match by `id::` uuid. Pass 2: LCS over the DFS sequence of unmatched blocks keyed by `(ancestor text path, text)`. Pass 3: pair remaining blocks at the same position with the same parent pair as "modified" when similarity ≥ 0.5 (character-level ratio). Shared by reload remap and 3-way merge.

## Acceptance Criteria
- Unit tests: insert at top, delete middle, edit one block, reorder siblings, duplicate texts, uuid-bearing block moved and edited.
- O(n·d) behaviour on 5,000-block pages (benchmark < 20 ms for small diffs).

## Notes
Story BIT-US-0068. Implements BIT-SP-0005.R13. [[block-editor]] §6.2–6.3.
