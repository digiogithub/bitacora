---
id: BIT-US-0068
type: story
title: Reload externally changed pages with BlockId remap and in-progress edit protection
status: done
priority: high
parent: BIT-EP-0008
milestone: BIT-M-0003
author: mcp
labels: [core, io, bitacora-core, bitacora-app]
estimate: 8
created: 2026-10-06T14:29:01Z
updated: 2026-10-06T21:12:42Z
started: 2026-10-06T19:42:38Z
closed: 2026-10-06T21:12:42Z
---

## Description
As a user with a page open while git pulls or Logseq edits it, I want the page to refresh in place keeping my scroll, selection and undo history, and never lose the text I am currently typing, so that external changes are harmless.

## Acceptance Criteria
- Block alignment `align(base, new)`: uuid (`id::`) first, then `(parent path, text)` LCS over DFS order; returns old→new id mapping plus added/removed sets.
- Clean page reload re-parses and remaps `BlockId`s; view state (scroll, selection, collapsed UI) and undo entries keep addressing the same blocks; recorded as non-undoable "External change".
- Undo history is not cleared; vanished targets truncate undo with a notice.
- If the edited block changed on disk: buffer kept, block marked conflicted, on commit choose keep mine / take disk / keep both; other blocks update without moving the caret.

## Notes
Implements: BIT-SP-0005.R13, BIT-SP-0005.R14.
See [[block-editor]] §4 (external reloads), §6.3; [[04-editor-outliner-operations]] §5.1 (Logseq has no read-side protection).
ADR-016: `align` should reuse the block matcher of `bitacora-merge` (BIT-T-0351) rather than a second implementation. ADR-017: base is the in-memory `DiskSnapshot`; after a restart a clean page is simply reloaded.
