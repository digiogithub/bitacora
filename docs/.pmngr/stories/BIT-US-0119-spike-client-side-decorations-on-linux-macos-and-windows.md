---
id: BIT-US-0119
type: story
title: "Spike: client-side decorations on Linux, macOS and Windows"
status: in_review
priority: high
parent: BIT-EP-0016
milestone: BIT-M-0006
author: mcp
labels: [v2, frameless, spike, bitacora-app]
estimate: 3
created: 2026-10-07T09:12:22Z
updated: 2026-10-07T10:22:16Z
started: 2026-10-07T10:22:16Z
---

## Description
As a developer, I want to validate `WindowDecorations::Client` + transparent titlebar with the pinned GPUI on each OS, and decide between gpui-component's `TitleBar` and a custom `AppTitleBar`, before restyling the shell.

## Acceptance Criteria
- Findings in `docs/design/frameless-window.md`: per-OS behaviour (GNOME/KDE Wayland, X11 with and without compositor, macOS, Windows) for move, resize, double-click, snap, traffic lights; unverified OSes flagged.
- Decision recorded (kit `TitleBar` vs custom) with the exact APIs used.

## Notes
Implements BIT-SP-0008.R4, BIT-SP-0008.R5 (investigation). APIs: `gpui-pre-0.3.8/src/platform.rs:734,2472,2500,2645`, `window.rs:2423,2446,2875,2888,2904,2919`; gpui-component 0.7.1 `title_bar.rs`, `window_border.rs`.
