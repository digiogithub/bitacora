---
id: BIT-T-0518
type: task
title: "Rename: rewrite page references inside {{query ...}} macros"
status: backlog
priority: medium
author: mcp
labels: [bitacora-core, rename, bug, logseq-compat]
created: 2026-11-12T16:36:55Z
updated: 2026-11-12T16:36:55Z
---

## Description
Found by the Logseq 0.10.15 golden test (`crates/bitacora-core/tests/rename_golden.rs`, BIT-T-0131). After renaming `Old` to `New Idea`, Logseq rewrites `{{query [[Old]]}}` to `{{query [[New Idea]]}}`. Bitacora's `rewrite_refs` (`crates/bitacora-core/src/rename.rs`) rewrites `{{embed [[Old]]}}` but leaves the query untouched, so the query silently stops matching.

## Acceptance Criteria
- Page references in `{{query ...}}` arguments follow the rename.
- The `pages/Refs.md` line 18 entry in `DIVERGENCES` of `rename_golden.rs` is removed and the test passes.

## Notes
BIT-SP-0002.R13.
