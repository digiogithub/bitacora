---
id: BIT-EP-0012
type: epic
title: Block-aware 3-way merge and visual conflict resolver
status: done
priority: high
milestone: BIT-M-0004
author: mcp
labels: [git, merge, ui]
created: 2026-10-06T14:21:13Z
updated: 2026-10-07T00:15:14Z
closed: 2026-10-07T00:15:14Z
---

## Description
Merge Logseq pages by block tree (match by `id::`, then structural/content similarity), auto-resolve metadata differences deterministically (`collapsed::`, `id::` additions, `:LOGBOOK:`, `card-*`, property order, whitespace, pure reorders), handle add/add journals, delete/modify, renames, special files (`config.edn`, `custom.css`, whiteboards, binary assets), and present content conflicts in a GPUI per-block resolver (ours / theirs / both / edit) — never writing conflict markers.

The page-level block-aware 3-way merge (model, matcher, field/metadata/structure merge, byte-preserving emit, `merge_page`, golden matrix) lives in the crate `crates/bitacora-merge` (depends only on `bitacora-markdown`) and is also used by `bitacora-core` for external edits (BIT-US-0069, M2). Git-specific parts (orchestration over trees, path policies incl. `config.edn`, renames, persisted merge state, rerere memo, external markers) stay in `bitacora-sync`.

## Acceptance Criteria
- Merge test matrix in [[git-sync-merge]] passes.
- No `<<<<<<<` marker is ever written to a graph file.
- User can resolve all conflicts of a sync from one screen; unresolved state survives restart.

## Notes
ADR-008, ADR-009, ADR-016. BIT-US-0049..BIT-US-0051 are needed by BIT-US-0069 (M2) ahead of this epic's M3 git work.
