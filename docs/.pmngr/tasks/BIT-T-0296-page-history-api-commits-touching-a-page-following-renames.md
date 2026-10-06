---
id: BIT-T-0296
type: task
title: "Page history API: commits touching a page following renames"
status: backlog
priority: low
parent: BIT-US-0048
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, history]
estimate: 2
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T14:32:57Z
---

## Description
`crates/bitacora-sync/src/history.rs`: `page_history(path, limit) -> Vec<HistoryEntry{oid, time, device, kind, subject, path_at_commit}>` walking first-parent + merge commits with gix, following renames via `diff_trees` rename detection; `page_at(oid, path) -> Vec<u8>`.

## Acceptance Criteria
- Tests on a temp repo with edits, a rename and a merge commit.

## Notes
Story BIT-US-0048. Implements BIT-SP-0006.R22.
