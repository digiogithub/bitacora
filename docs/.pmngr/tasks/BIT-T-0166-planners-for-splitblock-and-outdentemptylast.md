---
id: BIT-T-0166
type: task
title: Planners for SplitBlock and OutdentEmptyLast
status: backlog
priority: critical
parent: BIT-US-0032
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, outliner]
estimate: 3
created: 2026-10-06T14:30:44Z
updated: 2026-10-06T14:30:44Z
---

## Description
In `crates/bitacora-core/src/commands/split.rs` implement `plan_split(&Graph, id, cursor)`: head = text[..cursor] (hidden properties stay with the head via the projection), tail = text[cursor..] left-trimmed → `SetText(id, head)` + `InsertSubtree(new block)` as first child if `id` has children and is not collapsed, else next sibling; cursor==0 with non-empty text → `InsertSubtree(empty)` before. `plan_outdent_empty_last(&Graph, id)` → `Move` after parent when the block is empty and the last child. Set `cursor_after`.

## Acceptance Criteria
- Unit tests for the four scenarios of BIT-SP-0004.R6 at the op level.
- New block has no `id::` and no `collapsed::`.

## Notes
Story BIT-US-0032. Implements BIT-SP-0004.R6. Logseq `editor.cljs:421-569`, `:2240-2248`.
