---
id: BIT-US-0087
type: story
title: Page merge on rename collision
status: backlog
priority: medium
parent: BIT-EP-0009
milestone: BIT-M-0003
author: mcp
labels: [core, compat, rename]
estimate: 5
created: 2026-10-06T14:30:09Z
updated: 2026-10-06T14:30:09Z
---

## Description
As a user, I want renaming a page to the name of an existing page to merge both pages, so that I can consolidate duplicates without losing blocks, exactly as Logseq's `merge-pages!` does.

`merge-pages!` (`handler/page.cljs:568-616`): source blocks are moved to the end of the target, both files are rewritten, references to the source are updated to the target, and the source page/file is deleted (moved to `logseq/.recycle/`). Bitacora must confirm with the user before merging and keep `id::` of moved blocks.

## Acceptance Criteria
- `Foo` (2 blocks) → `Bar` (1 block): `pages/Bar.md` contains Bar's block followed by Foo's 2 blocks (canonical form, depth preserved), existing Bar bytes unchanged.
- `pages/Foo.md` moved to `logseq/.recycle/pages_Foo.md`.
- `[[Foo]]`/`#Foo` refs become `[[Bar]]`/`#Bar` graph-wide.
- Block `id::` lines of moved blocks are kept; block refs to them still resolve.
- Foo's page properties (pre-block) are not appended as a block; aliases from Foo are merged into Bar's `alias::` only if the user opts in (default: dropped with a warning listing them).
- Merge is one undoable transaction; UI asks for confirmation.

## Notes
Implements: BIT-SP-0002.R13, BIT-SP-0002.R14
See [[01-file-graph-layout]] §10.2; [[04-editor-outliner-operations]]; [[block-editor]]. ADR-006, ADR-011.
