---
id: BIT-T-0353
type: task
title: Content diff3 within a block and per-key user property merge
status: backlog
priority: critical
parent: BIT-US-0050
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, merge]
estimate: 3
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T14:34:49Z
---

## Description
`crates/bitacora-sync/src/merge/fields.rs`: `merge_content(b, o, t) -> FieldResult<String>` (short-circuit equalities, else line diff3 using `diffy` / `imara-diff` merge on normalized lines; overlapping hunks → `Conflict::Content` with output = ours); `merge_user_props(b, o, t)` per key 3-way (add/remove/change), same key divergent → `Conflict::Property{key}`; output order = ours order + new theirs keys in theirs' relative order.

## Acceptance Criteria
- Tests: different-line edits merge; same-line edit conflicts; property scenarios from BIT-SP-0006.R10.

## Notes
Story BIT-US-0050. Implements BIT-SP-0006.R10, BIT-SP-0006.R11.
