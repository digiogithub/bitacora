---
id: BIT-M-0006
type: milestone
title: M5 — v2 Design system & frameless shell
status: backlog
author: mcp
labels: [v2]
created: 2026-10-07T09:07:50Z
updated: 2026-10-07T09:07:50Z
due: 2026-12-18
---

## Description
First v2 checkpoint: the new design system (tokens, fonts, theme, component kit) is the only source of UI values, the window is frameless with an app-drawn 52px top bar and window controls, and the main screens follow the new mockups.

## Acceptance Criteria
- Tokens pipeline generates the Rust palette reproducibly; contrast check passes in CI.
- Window opens without OS title bar on Linux (Wayland + X11), macOS and Windows; move, resize, double-click maximize, minimize/maximize/close work (or documented fallback).
- Top bar, left sidebar with calendar, outline/journal styling, right panel, tasks view and popovers match the design in dark and light.
- All existing tests stay green; no Op/core behaviour changes.

## Notes
Plan: [[bitacora-v2-plan]]. Epics: design system foundation, frameless window, v2 screen redesign.
