---
id: BIT-T-0231
type: task
title: Selection model and keyboard/mouse selection gestures
status: backlog
priority: high
parent: BIT-US-0036
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, outliner]
estimate: 3
created: 2026-10-06T14:31:33Z
updated: 2026-10-06T14:31:33Z
---

## Description
`crates/bitacora-app/src/editor/selection.rs`: `Selection { ids: IndexSet<BlockId>, anchor }` with `top_level(&Graph)`; gestures: `Esc` from edit mode selects the block; `Shift+Up/Down` extend/shrink over visible DFS order; `Shift+click` range; `Mod+Shift+A` select all; `Mod+A` select parent; `Enter` edits a single selection; `Esc` clears. Highlight rendering in `PageView`. Selection remapped through `BlockId` remap on reload.

## Acceptance Criteria
- Unit tests for `top_level` and range extension across depths.
- `#[gpui::test]` for Esc → Shift+Down selects two blocks.

## Notes
Story BIT-US-0036. Implements BIT-SP-0004.R12.
