---
id: BIT-EP-0016
type: epic
title: Frameless window and app-drawn title bar
status: backlog
priority: high
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, frameless, bitacora-app]
created: 2026-10-07T09:10:26Z
updated: 2026-10-07T09:10:26Z
---

## Description
Draw the whole window without the OS title bar: `WindowDecorations::Client`, transparent titlebar on macOS with inset traffic lights, a custom 52px `AppTitleBar` (drag area via `WindowControlArea::Drag`, minimize/maximize/close, double-click maximize, right-click window menu on Linux), resize edges (`window_border`, `start_window_resize`), and graceful fallback when the platform forces server decorations.

## Acceptance Criteria
- BIT-SP-0008.R4 and R5 satisfied, verified manually per OS with a checklist.
- No duplicate controls under server decorations.

## Notes
- Plan [[bitacora-v2-plan]] decision D7 (ADR-033). APIs verified in `gpui-pre-0.3.8/src/platform.rs:2472,2500,2645`, `window.rs:2446,2875,2888,2919`, `div.rs:1257`; gpui-component 0.7.1 `title_bar.rs`, `window_border.rs`, `root.rs:457`.
- Risks: GNOME Wayland (no SSD), X11 without compositor, Windows hit-test inside interactive bar children.
