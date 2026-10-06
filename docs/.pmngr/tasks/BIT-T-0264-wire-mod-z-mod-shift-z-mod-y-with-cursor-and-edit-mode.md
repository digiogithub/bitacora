---
id: BIT-T-0264
type: task
title: Wire Mod+Z / Mod+Shift+Z / Mod+Y with cursor and edit-mode restore
status: done
priority: high
parent: BIT-US-0039
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, undo]
estimate: 2
created: 2026-10-06T14:32:23Z
updated: 2026-10-06T21:12:28Z
closed: 2026-10-06T21:12:28Z
---

## Description
Bind undo/redo actions in all contexts; flush the edit buffer first; after undo/redo restore `CursorState` (block, selection range, edit vs selection mode), scrolling the block into view; show the truncation notice when returned.

## Acceptance Criteria
- `#[gpui::test]`: split then undo restores edit mode in the original block at the original offset.
- Undo while typing first commits the buffer then undoes the coalesced typing.

## Notes
Story BIT-US-0039. Implements BIT-SP-0004.R17. Logseq `history.cljs:10-51`.
