---
id: BIT-US-0106
type: story
title: Drag and drop of blocks with before/child/after zones
status: done
priority: medium
parent: BIT-EP-0013
milestone: BIT-M-0005
author: mcp
labels: [ui, editor, bitacora-app]
estimate: 5
created: 2026-10-06T14:31:46Z
updated: 2026-10-06T22:30:47Z
started: 2026-10-06T22:00:23Z
closed: 2026-10-06T22:30:47Z
---

## Description
As a writer, I want to drag a block (or a selection of blocks) by its bullet and drop it before, after or as a child of another block, even on another page or in the sidebar, so that I can reorganise my outline with the mouse.

## Acceptance Criteria
- Draggable bullets using GPUI `on_drag`/`on_drop`; drop indicator for before / as child / after based on pointer x/y.
- Multi-block selection drags move all selected blocks preserving order.
- Alt-drop inserts a `((uuid))` ref instead of moving (writes `id::` on the source).
- Cross-page and right-sidebar drops produce one multi-file transaction (one undo step).
- Auto-scroll near viewport edges.

## Notes
[[04-editor-outliner-operations]] Requirements 17; [[block-editor]] §7.5; [[gpui-and-gpui-kit]] §2.3 (Bullets, drag and drop). ADR-006.
