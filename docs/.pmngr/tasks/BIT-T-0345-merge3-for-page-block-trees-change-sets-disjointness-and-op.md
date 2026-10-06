---
id: BIT-T-0345
type: task
title: "merge3 for page block trees: change sets, disjointness and op generation"
status: backlog
priority: high
parent: BIT-US-0069
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, merge]
estimate: 5
created: 2026-10-06T14:34:14Z
updated: 2026-10-06T15:08:58Z
---

## Description
`crates/bitacora-core/src/external/merge3.rs`: `merge3(base, theirs, ours: &Page) -> MergeOutcome`, a thin adapter over `bitacora_merge::merge_page` (crate `crates/bitacora-merge`, ADR-016). Serialize ours (or build the merge model from it), call `merge_page(base_bytes, ours_bytes, theirs_bytes)`, and translate the result back into core terms: per block {text edit, delete, insert at (parent, after), move} from theirs becomes a `Vec<Op>` applied to ours (inserts ordered theirs-first at the same anchor), using the block pairing returned by `bitacora-merge` to map onto existing `BlockId`s. Clean when `bitacora-merge` reports no conflicts; otherwise `Conflict(Vec<BlockConflict { kind, base, ours, theirs }>)` mapped from its conflict records. Hidden-property-only differences (e.g. `collapsed::`) follow ADR-009 auto-resolution rules, which live in `bitacora-merge`. No merge algorithm is duplicated in core.

## Acceptance Criteria
- Pure function, no I/O; unit tests for each change kind combination through the adapter.
- Never produces conflict markers.

## Notes
Story BIT-US-0069. Implements BIT-SP-0005.R15. ADR-008, ADR-009, ADR-016. Depends on the `bitacora-merge` page merge (BIT-US-0049..BIT-US-0051).
