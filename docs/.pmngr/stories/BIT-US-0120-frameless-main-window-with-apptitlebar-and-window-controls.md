---
id: BIT-US-0120
type: story
title: Frameless main window with AppTitleBar and window controls
status: in_review
priority: high
parent: BIT-EP-0016
milestone: BIT-M-0006
author: mcp
labels: [v2, frameless, bitacora-app]
estimate: 5
created: 2026-10-07T09:12:22Z
updated: 2026-10-07T10:39:17Z
started: 2026-10-07T10:39:17Z
---

## Description
As a user, I want the app to draw its own top bar with minimize, maximize and close, so that the window looks like the design and has no OS title bar.

## Acceptance Criteria
- `app.rs` opens windows with `window_decorations: Some(WindowDecorations::Client)` and transparent titlebar; window title still set for taskbar/alt-tab.
- 52px `AppTitleBar`: drag area (`WindowControlArea::Drag`), min/max/close buttons (Linux/Windows) honouring `window_controls()`, double-click maximize, Linux right-click window menu, macOS traffic-light inset (~78px reserved).
- Under `Decorations::Server` no controls are drawn; close goes through the existing close/tray flow.

## Notes
Implements BIT-SP-0008.R4, BIT-SP-0008.R5. Depends on the CSD spike. Re-export `WindowDecorations`, `Decorations`, `WindowControlArea`, `ResizeEdge` via `crate::ui`.
