---
id: BIT-T-0124
type: task
title: Implement get_graph_info, list_graphs and git_sync_status tools
status: in_progress
priority: medium
parent: BIT-US-0018
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, tools, read]
estimate: 2
created: 2026-10-06T14:29:55Z
updated: 2026-10-06T18:44:25Z
started: 2026-10-06T18:44:25Z
---

## Description
`crates/bitacora-mcp/src/tools/graph.rs`: `get_graph_info` (name, path, page/block counts, date format, journals dir), `list_graphs` (known graphs from app settings), `git_sync_status` reading a `watch::Receiver<SyncStatus>` exposed through `CoreFacade` (returns `{state:"Disabled"}` when sync not configured; later populated by `bitacora-sync`). Define `SyncStatus` DTO consistent with [[git-sync-merge]] §6 (state, ahead, behind, last_sync, conflicts {count, pages}, last_error).

## Acceptance Criteria
- Tests with a stub sync status provider for Disabled, Idle, Conflicted.

## Notes
Story BIT-US-0018. Implements BIT-SP-0007.R11.
