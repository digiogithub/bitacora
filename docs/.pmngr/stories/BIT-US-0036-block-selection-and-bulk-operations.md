---
id: BIT-US-0036
type: story
title: Block selection and bulk operations
status: backlog
priority: high
parent: BIT-EP-0007
milestone: BIT-M-0003
author: mcp
labels: [editor, outliner, bitacora-app, bitacora-core]
estimate: 5
created: 2026-10-06T14:28:07Z
updated: 2026-10-06T14:28:07Z
---

## Description
As an outliner user, I want to select several blocks with the keyboard or mouse and delete, indent, outdent, move, cycle TODO, copy or cut them at once, so that I can reorganise whole sections in one undoable step.

## Acceptance Criteria
- `Esc` from edit mode selects the edited block; `Shift+Up/Down` extend in DFS order of visible blocks; `Shift+click` range; `Mod+Shift+A` all; `Mod+A` select parent; `Enter` edits a single selected block; `Esc` clears.
- Selection model = set of `BlockId`s + anchor; operations act on top-level blocks only.
- `Backspace`/`Delete` on selection plans `DeleteBlocks` (one transaction).
- `Tab`, `Shift+Tab`, `Alt+Shift+Up/Down`, `Mod+Enter` act on the selection as one transaction.
- Selection is rendered (highlight) and survives reloads via BlockId remap.

## Notes
Implements: BIT-SP-0004.R12.
See [[block-editor]] §7.3, §7.5; [[04-editor-outliner-operations]] §3 (Select blocks, Multi-block ops). Clipboard handled in its own story.
