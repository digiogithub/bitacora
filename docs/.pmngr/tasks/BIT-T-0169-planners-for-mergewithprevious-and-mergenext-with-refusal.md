---
id: BIT-T-0169
type: task
title: Planners for MergeWithPrevious and MergeNext with refusal rules
status: backlog
priority: critical
parent: BIT-US-0033
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, outliner]
estimate: 3
created: 2026-10-06T14:30:44Z
updated: 2026-10-06T14:30:44Z
---

## Description
In `crates/bitacora-core/src/commands/merge.rs`: `plan_merge_prev(id)` finds the previous visible block, emits `SetText(prev, prev_text + visible text of id)` (hidden properties of `id` handled by the uuid rule), `AdoptChildren(id → prev)`, `RemoveSubtree(id)`; `cursor_after` = len(prev visible). Refusals: both have children; first block of page unless empty (empty → plain delete). `plan_merge_next(id)` pulls first child or next sibling; refused if it has children.

## Acceptance Criteria
- Op-level tests for merge, both-children refusal, first-block rules, Delete@end.
- Merged text joins without inserting spaces (Logseq behaviour).

## Notes
Story BIT-US-0033. Implements BIT-SP-0004.R7. Logseq `editor.cljs:790-869`, `:2659-2702`.
