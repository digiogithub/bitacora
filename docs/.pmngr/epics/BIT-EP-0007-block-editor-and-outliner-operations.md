---
id: BIT-EP-0007
type: epic
title: Block editor and outliner operations
status: in_review
priority: critical
milestone: BIT-M-0003
author: mcp
labels: [editor, ui, core]
created: 2026-10-06T14:21:13Z
updated: 2026-10-07T00:15:15Z
started: 2026-10-07T00:15:15Z
---

## Description
The custom GPUI block editor and the `Op`-based outliner from [[block-editor]]: click-to-edit with caret placement, Enter split, Shift+Enter, Backspace/Delete merge, Tab/Shift+Tab, Alt+Shift+Up/Down, collapse persistence, multi-block selection and bulk ops, copy/cut/paste (Markdown → blocks), Mod+Enter TODO cycling, zoom, `[[` / `#` / `((` autocomplete with `id::` generation, undo/redo with typing coalescing, IME.

## Acceptance Criteria
- MVP feature list of [[block-editor]] §9 implemented and covered by op-level tests.
- Undo/redo restores exact bytes on disk.
- Keyboard shortcuts match Logseq defaults for the MVP set.

## Notes
ADR-002, ADR-006. See [[04-editor-outliner-operations]].
