---
id: BIT-T-0363
type: task
title: "sync_merge orchestrator: per-path merge to a merged tree"
status: backlog
priority: critical
parent: BIT-US-0053
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, merge]
estimate: 3
created: 2026-10-06T14:34:50Z
updated: 2026-10-06T14:34:50Z
---

## Description
`crates/bitacora-sync/src/merge/sync_merge.rs`: `sync_merge(backend, head, remote, ctx) -> SyncMergeOutcome{ tree: Oid, changed_paths, conflicts, notes }`: merge-base; union of changed paths from both `diff_trees` (rename-aware); read b/o/t blobs; dispatch by policy; collect outputs into `TreeEdit`s on HEAD's tree; `write_tree`. Clean → caller creates merge commit with message `Kind: merge` listing info notes; conflicts → caller persists state. Must be pure w.r.t. work tree (no file writes here).

## Acceptance Criteria
- Tests with temp repos: clean divergence yields expected tree; conflicts reported with stable ids.

## Notes
Story BIT-US-0053. Implements BIT-SP-0006.R4, BIT-SP-0006.R15. See [[git-sync-merge]] §4.4.
