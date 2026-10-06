---
id: BIT-T-0353
type: task
title: Content diff3 within a block and per-key user property merge
status: in_progress
priority: critical
parent: BIT-US-0050
milestone: BIT-M-0003
author: mcp
labels: [bitacora-merge, merge]
estimate: 3
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T17:08:27Z
started: 2026-10-06T17:08:27Z
---

## Description
`crates/bitacora-merge/src/fields.rs`: `merge_content(b, o, t) -> FieldResult<String>` (short-circuit equalities, else line diff3 using `diffy` / `imara-diff` merge on normalized lines; overlapping hunks → `Conflict::Content` with output = ours); `merge_user_props(b, o, t)` per key 3-way (add/remove/change), same key divergent → `Conflict::Property{key}`; output order = ours order + new theirs keys in theirs' relative order.

## Acceptance Criteria
- Tests: different-line edits merge; same-line edit conflicts; property scenarios from BIT-SP-0006.R10.

## Notes
Story BIT-US-0050. Implements BIT-SP-0006.R10, BIT-SP-0006.R11.
ADR-016: lives in the `bitacora-merge` crate (depends only on `bitacora-markdown`), shared by `bitacora-core` (external edits, BIT-US-0069) and `bitacora-sync`.
Needed by BIT-US-0069 (ADR-016).
