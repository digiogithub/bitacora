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
updated: 2026-10-06T14:34:14Z
---

## Description
`crates/bitacora-core/src/external/merge3.rs`: `merge3(base, theirs, ours: &Page) -> MergeOutcome`. Compute change sets via `align(base, theirs)` and `align(base, ours)`: per block {text edit, delete, insert at (parent, after), move}. Clean when no block is changed on both sides (identical changes count as no conflict) and inserts/moves don't target a block deleted by the other side. Output: `Vec<Op>` applying theirs' changes to ours (inserts ordered theirs-first at the same anchor). Otherwise `Conflict(Vec<BlockConflict { kind, base, ours, theirs }>)`. Hidden-property-only differences (e.g. `collapsed::`) follow ADR-009 auto-resolution rules where both sides touched only metadata.

## Acceptance Criteria
- Pure function, no I/O; unit tests for each change kind combination.
- Never produces conflict markers.

## Notes
Story BIT-US-0069. Implements BIT-SP-0005.R15. ADR-008, ADR-009. Coordinate with the sync merge in BIT-EP-0012 (shared module or common trait).
