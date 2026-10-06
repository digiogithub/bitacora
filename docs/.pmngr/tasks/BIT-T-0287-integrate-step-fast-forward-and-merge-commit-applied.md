---
id: BIT-T-0287
type: task
title: "Integrate step: fast-forward and merge commit applied through the core writer"
status: backlog
priority: critical
parent: BIT-US-0045
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, bitacora-core, state-machine]
estimate: 3
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T14:32:57Z
---

## Description
`crates/bitacora-sync/src/engine/integrate.rs`: after fetch, compare HEAD/remote with `merge_base`: ahead-only → Pushing; behind → fast-forward: request `flush_all`, take write lock, compute changed paths (`diff_trees`), apply new contents via core `ApplyExternalFiles` (writer → index → editors reload; undo boundary, clear redo for affected pages), then `update-ref refs/heads/<b>` with CAS and `git read-tree`/index refresh; diverged → call `bitacora_sync::merge::sync_merge` (BIT-EP-0012), on clean result `commit_tree([HEAD, REMOTE])` + `update_ref` + apply files; conflicts → hand off to conflict state. Wait for pending core write transactions before starting.

## Acceptance Criteria
- Tests with FakeBackend + in-memory core: FF updates files and index; merge commit has two parents; sync waits for pending transaction.

## Notes
Story BIT-US-0045. Implements BIT-SP-0006.R4, BIT-SP-0006.R7, BIT-SP-0006.R14. ADR-011.
