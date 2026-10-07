---
id: BIT-T-0396
type: task
title: Move dock tabs into the title bar with design tab style
status: in_review
priority: high
parent: BIT-US-0122
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, dock]
estimate: 5
created: 2026-10-07T09:13:45Z
updated: 2026-10-07T11:19:45Z
started: 2026-10-07T11:19:41Z
---

## Description
Render the center dock tab strip inside the title bar (custom tab bar over the existing dock state or kit TabPanel styling), active-tab style from tokens, overflow handling, middle-click close, drag reorder; persisted layout format unchanged or migrated.

## Acceptance Criteria
- Existing tab/dock tests pass; layout from 1.x restores; dragging a tab does not move the window.
