---
id: BIT-T-0199
type: task
title: Toggle and keyboard focus for right sidebar items
status: in_progress
priority: low
parent: BIT-US-0080
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui]
estimate: 1
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T19:14:39Z
started: 2026-10-06T19:14:39Z
---

## Description
Wire `t r` (`ToggleRightSidebar`) and add keyboard focus cycling between main view and sidebar items (Mod+Shift+Left/Right as focus pane actions, configurable), plus "Close all" action.

## Acceptance Criteria
- `#[gpui::test]`: toggle hides/shows panel without losing stack; focus cycling reaches each item.

## Notes
Epic AC: keyboard-only navigation.
