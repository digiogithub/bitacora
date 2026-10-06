---
id: BIT-US-0039
type: story
title: Undo/redo with typing coalescing, cursor restore and byte-exact files
status: done
priority: critical
parent: BIT-EP-0007
milestone: BIT-M-0003
author: mcp
labels: [editor, undo, bitacora-core, bitacora-app]
estimate: 5
created: 2026-10-06T14:28:07Z
updated: 2026-10-06T21:12:42Z
started: 2026-10-06T20:22:06Z
closed: 2026-10-06T21:12:42Z
---

## Description
As a user who makes mistakes, I want Mod+Z / Mod+Shift+Z to undo and redo whole actions (and sensible chunks of typing) and put my caret back where it was, with the file on disk returning to exactly its previous bytes, so that I can experiment safely.

## Acceptance Criteria
- Graph-wide `History` stack capped at ~1,000 entries / 50 MB captured text.
- Undo flushes the buffer, applies reversed inverses, restores `cursor_before`; redo restores `cursor_after`; any new transaction clears redo.
- Typing coalescing: same block, no structural op, < 1.5 s between keystrokes, no word-boundary-after-pause.
- Undo/redo go through the write path; after undo the file is byte-identical to before the transaction (including CRLF and odd indentation).
- External reloads keep history; vanished targets stop undo with "history truncated by external change".

## Notes
Implements: BIT-SP-0004.R17, BIT-SP-0004.R18, BIT-SP-0004.R5.
See [[block-editor]] §4, [[04-editor-outliner-operations]] §6. Relates to BIT-SP-0005.R13 (BlockId remap on reload).
