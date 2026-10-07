---
id: BIT-T-0491
type: task
title: "Graph menu: Open graph…, Open recent, Close graph"
status: backlog
priority: high
parent: BIT-US-0165
milestone: BIT-M-0006
author: mcp
labels: [v2, ux, bitacora-app]
estimate: 1
created: 2026-10-07T09:55:34Z
updated: 2026-10-07T09:55:34Z
---

## Description
Add actions and menu entries:
- native File menu on macOS (`set_menus`);
- in-window app menu / sidebar footer switcher on Linux and Windows;
- the existing palette `SwitchGraph`.

"Open recent" lists `RecentGraphs`.

## Acceptance Criteria
- `#[gpui::test]`: each action dispatches the expected `open_graph` or picker.
