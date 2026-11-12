---
id: BIT-T-0517
type: task
title: "Rename: move pages whose title nests the renamed reference"
status: backlog
priority: medium
author: mcp
labels: [bitacora-core, rename, bug, logseq-compat]
created: 2026-11-12T16:36:45Z
updated: 2026-11-12T16:36:45Z
---

## Description
Found by the Logseq 0.10.15 golden test (`crates/bitacora-core/tests/rename_golden.rs`, BIT-T-0131). Renaming `Old` to `New Idea` when a page titled `Notes on [[Old]]` exists: Logseq moves that page to `Notes on [[New Idea]]`. Bitacora rewrites the links (`[[Notes on [[Old]]]]` becomes `[[Notes on [[New Idea]]]]`) but leaves the file at `pages/Notes on [[Old]].md`, so the rewritten links point at a page without a file.

## Acceptance Criteria
- Pages whose title contains `[[old]]` are moved in the same transaction (needs a title lookup in `RefLookup`, implemented by `bitacora-runtime`'s `IndexRefLookup`).
- The entry in `PATH_DIVERGENCES` of `rename_golden.rs` is removed and the test passes.

## Notes
BIT-SP-0002.R13.
