---
id: BIT-T-0162
type: task
title: Define editor actions, four key contexts and Logseq default keymap
status: backlog
priority: high
parent: BIT-US-0031
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, editor, keymap]
estimate: 3
created: 2026-10-06T14:30:44Z
updated: 2026-10-06T14:30:44Z
---

## Description
In `crates/bitacora-app/src/keymap/` declare GPUI actions for all MVP commands (`editor::NewBlock`, `NewLine`, `Indent`, `Outdent`, `MoveUp`, `MoveDown`, `CycleTodo`, `Collapse`, `Expand`, `ToggleCollapse`, `ZoomIn`, `ZoomOut`, `Undo`, `Redo`, `Copy`, `Cut`, `Paste`, `PasteRaw`, `CopyEmbed`, `SelectUp/Down`, `SelectAll`, `SelectParent`, `Escape`, `autocomplete::Confirm/Prev/Next/Close`, …) and the key contexts `Outliner`, `BlockSelection`, `BlockEditor`, `Autocomplete`. Default bindings table per platform from [[04-editor-outliner-operations]] §7 (`Mod` = cmd on macOS, ctrl elsewhere; move block `cmd-shift-up` on mac vs `alt-shift-up`).

## Acceptance Criteria
- `#[gpui::test]`: `Enter` with popup open dispatches `autocomplete::Confirm`, not `editor::NewBlock`.
- Snapshot test of the resolved default keymap per platform.

## Notes
Story BIT-US-0031. Implements BIT-SP-0004.R21.
