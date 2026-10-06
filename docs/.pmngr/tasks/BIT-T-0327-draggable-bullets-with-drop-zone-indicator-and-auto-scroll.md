---
id: BIT-T-0327
type: task
title: Draggable bullets with drop-zone indicator and auto-scroll
status: backlog
priority: medium
parent: BIT-US-0106
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, editor, ui]
estimate: 3
created: 2026-10-06T14:34:02Z
updated: 2026-10-06T14:34:02Z
---

## Description
`crates/bitacora-app/src/editor/dnd.rs`: make bullets draggable with GPUI `on_drag` (drag payload `DraggedBlocks { uuids, source_page }`, ghost preview showing the first block title + count); each `BlockView` handles `drag_over`/`on_drop` computing the zone from pointer position: top third → before, bottom third with x beyond content indent → child, else after; render a theme-coloured indicator line (indented for child). Auto-scroll the `list` when the pointer is within 40 px of the viewport edge. Dropping into its own subtree is rejected.

## Acceptance Criteria
- Unit tests for zone computation.
- `#[gpui::test]`: simulated drag of block B before A dispatches `MoveBlocks { target, position: Before }`; drop onto own child is a no-op.

## Notes
[[block-editor]] §7.5; [[gpui-and-gpui-kit]] §2.3.
