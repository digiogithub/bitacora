---
id: BIT-EP-0012
type: epic
title: Block-aware 3-way merge and visual conflict resolver
status: backlog
priority: high
milestone: BIT-M-0004
author: mcp
labels: [git, merge, ui]
created: 2026-10-06T14:21:13Z
updated: 2026-10-06T14:21:13Z
---

## Description
Merge Logseq pages by block tree (match by `id::`, then structural/content similarity), auto-resolve metadata differences deterministically (`collapsed::`, `id::` additions, `:LOGBOOK:`, `card-*`, property order, whitespace, pure reorders), handle add/add journals, delete/modify, renames, special files (`config.edn`, `custom.css`, whiteboards, binary assets), and present content conflicts in a GPUI per-block resolver (ours / theirs / both / edit) — never writing conflict markers.

## Acceptance Criteria
- Merge test matrix in [[git-sync-merge]] passes.
- No `<<<<<<<` marker is ever written to a graph file.
- User can resolve all conflicts of a sync from one screen; unresolved state survives restart.

## Notes
ADR-008, ADR-009.
