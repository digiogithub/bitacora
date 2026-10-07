---
id: BIT-T-0392
type: task
title: macOS traffic-light inset and server-decoration fallback
status: backlog
priority: high
parent: BIT-US-0120
milestone: BIT-M-0006
author: mcp
labels: [v2, frameless, macos, linux]
estimate: 2
created: 2026-10-07T09:13:09Z
updated: 2026-10-07T09:13:09Z
---

## Description
Position traffic lights with `traffic_light_position` so they sit vertically centred in the 52px bar before the nav buttons, reserve left space on macOS only; when `window_decorations()` is `Server` (X11 without compositor) hide app-drawn controls; react to decoration changes at runtime; route close through the existing quit/tray flow.

## Acceptance Criteria
- Unit test of control visibility per `Decorations`/`WindowControls` combination; macOS manual check recorded.
