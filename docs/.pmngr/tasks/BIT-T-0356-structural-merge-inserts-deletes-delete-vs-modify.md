---
id: BIT-T-0356
type: task
title: "Structural merge: inserts, deletes, delete-vs-modify, reparenting and sibling order"
status: backlog
priority: critical
parent: BIT-US-0051
milestone: BIT-M-0003
author: mcp
labels: [bitacora-merge, merge]
estimate: 3
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T15:17:52Z
---

## Description
`crates/bitacora-merge/src/structure.rs`: classify triples (`Unchanged`, `OnlyOurs`, `OnlyTheirs`, `Same`, `BothChanged`, `InsertedOurs/Theirs/Both`, `DeletedOurs/Theirs/Both`, `DeletedVsModified`); compute merged parent for each block via 3-way on (parent key): one side moved → take it, both moved differently → ours + `Note::MoveConflictTookOurs`; orphan (parent deleted on other side) → nearest surviving ancestor + note; sibling order via 3-way list merge of keys (ours wins overlaps); both-side inserts at same position → ours first, dedupe equal normalized content; delete-vs-modify (also when only a descendant modified) → keep modified block in output + `Conflict::DeleteVsModify`.

## Acceptance Criteria
- Tests for each row of the structure-level table in [[git-sync-merge]] §4.3.

## Notes
Story BIT-US-0051. Implements BIT-SP-0006.R12.
ADR-016: lives in the `bitacora-merge` crate (depends only on `bitacora-markdown`), shared by `bitacora-core` (external edits, BIT-US-0069) and `bitacora-sync`.
Needed by BIT-US-0069 (ADR-016).
