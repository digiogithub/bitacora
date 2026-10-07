---
id: BIT-T-0391
type: task
title: AppTitleBar with drag area, window controls and double-click
status: backlog
priority: high
parent: BIT-US-0120
milestone: BIT-M-0006
author: mcp
labels: [v2, frameless]
estimate: 3
created: 2026-10-07T09:13:08Z
updated: 2026-10-07T09:13:08Z
---

## Description
`views/title_bar.rs`: 52px bar in `side` token, drag area via `window_control_area(Drag)` + `start_window_move`, min/max/close buttons with `WindowControlArea::{Min,Max,Close}` (native hit-test on Windows), double-click `zoom_window` (Linux) / `titlebar_double_click` (macOS), Linux right-click `show_window_menu`. Interactive children never start a drag.

## Acceptance Criteria
- `#[gpui::test]` for layout and button actions; manual check on Linux Wayland and X11.
