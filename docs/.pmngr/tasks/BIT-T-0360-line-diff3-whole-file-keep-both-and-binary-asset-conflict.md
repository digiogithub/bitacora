---
id: BIT-T-0360
type: task
title: Line diff3, whole-file keep-both and binary asset conflict copies
status: done
priority: high
parent: BIT-US-0052
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, merge, assets]
estimate: 2
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T18:59:52Z
closed: 2026-10-06T18:59:52Z
---

## Description
`crates/bitacora-sync/src/merge/files.rs`: `merge_text_lines` (css/js/.gitignore/other text; overlap → `Conflict::Text` with ours output; built on `bitacora_merge::diff3_lines`, the same line diff3 that `merge_page` uses as fallback, ADR-016); `merge_whole_file_keep_both` (whiteboards `.edn`/`.excalidraw`/`.tldr`: one side changed → take it; both → ours at path + theirs at `<stem> (conflict <device> <yyyy-mm-dd>).<ext>`, `Note`); `merge_binary` (identical no-op; one side → take; both → ours + `<stem> (conflict-<short-sha>).<ext>`). Ignore policy → take ours.

## Acceptance Criteria
- Tests for each policy including asset collision naming per BIT-SP-0006.R17.

## Notes
Story BIT-US-0052. Implements BIT-SP-0006.R17. ADR-016: git file-level policies stay in `bitacora-sync`; line diff3 comes from `bitacora-merge`.
