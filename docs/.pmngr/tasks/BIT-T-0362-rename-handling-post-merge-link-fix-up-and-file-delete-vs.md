---
id: BIT-T-0362
type: task
title: Rename handling, post-merge link fix-up and file delete-vs-modify
status: backlog
priority: high
parent: BIT-US-0052
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, merge, rename]
estimate: 3
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T14:34:49Z
---

## Description
`crates/bitacora-sync/src/merge/renames.rs`: from both `diff_trees` results build a path map base→ours/theirs; rename on one side + modify on other → `merge_page` at the renamed path; rename/rename → `Conflict::RenameRename{ours_title, theirs_title}` (output: ours path, suggestion adds `alias::` of the other title); post-merge fix-up: for pages renamed (title A→B) on one side, rewrite `[[A]]`, `#A`, `#[[A]]`, `tags::`/`alias::` refs introduced by the other side in merged outputs (via core link-rewrite helper), add `Note::LinksRewritten`. File deleted one side + modified other → `Conflict::FileDeleteVsModify` (default suggestion restore; output keeps modified file). Case-only renames applied via temp name when writing the work tree.

## Acceptance Criteria
- Tests for the three scenarios of BIT-SP-0006.R19 and delete-vs-modify both directions.

## Notes
Story BIT-US-0052. Implements BIT-SP-0006.R19. See [[git-sync-merge]] §5.5–5.6, [[01-file-graph-layout]].
