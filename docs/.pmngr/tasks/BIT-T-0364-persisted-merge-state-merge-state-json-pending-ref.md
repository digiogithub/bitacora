---
id: BIT-T-0364
type: task
title: "Persisted merge state: merge-state.json, pending ref, resolution and resolve commit"
status: backlog
priority: critical
parent: BIT-US-0053
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, merge, conflicts]
estimate: 3
created: 2026-10-06T14:34:50Z
updated: 2026-10-06T14:34:50Z
---

## Description
`crates/bitacora-sync/src/merge/state.rs`: `MergeState { base, ours, theirs, merged_tree, conflicts: Vec<ConflictRecord> }` (schema from [[git-sync-merge]] §4.5, versioned) saved atomically to `.git/bitacora/merge-state.json` and `refs/bitacora/pending-merge` → merged tree commit. API: `resolve(conflict_id, Resolution{Ours|Theirs|Both|Edit(String)|Keep|Delete})`, `apply_resolutions()` → re-emit affected pages, write via core writer; when all resolved → `commit_tree([local HEAD, theirs])` with `Kind: resolve`, update ref, push, delete state. While conflicted: local auto commits continue on top of ours; push suppressed; writes to conflicting blocks return `BlockInConflict` to core/MCP.

## Acceptance Criteria
- Tests: save/load round-trip; restart restores state; final resolve commit parents and kind; no push before resolution.

## Notes
Story BIT-US-0053. Implements BIT-SP-0006.R15, BIT-SP-0007.R10.
