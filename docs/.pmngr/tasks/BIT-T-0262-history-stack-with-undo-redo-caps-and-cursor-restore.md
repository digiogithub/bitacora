---
id: BIT-T-0262
type: task
title: History stack with undo/redo, caps and cursor restore
status: backlog
priority: critical
parent: BIT-US-0039
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, undo]
estimate: 3
created: 2026-10-06T14:32:23Z
updated: 2026-10-06T14:32:23Z
---

## Description
`crates/bitacora-core/src/history.rs`: `History { undo: VecDeque<Transaction>, redo: Vec<Transaction>, bytes }` capped at 1,000 entries / 50 MB of captured text (evict oldest). `Graph::undo()` flushes, applies `ops.iter().rev().map(inverse)` as a new write-path change (pages dirty), returns `cursor_before`; `redo()` re-applies, returns `cursor_after`. New commit clears redo. Non-undoable transactions (External change) are skipped but can invalidate entries: if an op target `BlockId` no longer exists, undo stops with `UndoError::HistoryTruncated` and drops older entries.

## Acceptance Criteria
- Tests: undo/redo of every command type; redo cleared by new commit; caps enforced.
- Truncation path returns the notice and leaves the graph unchanged.

## Notes
Story BIT-US-0039. Implements BIT-SP-0004.R17. Logseq `undo_redo.cljs:247-272`.
