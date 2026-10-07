---
id: BIT-T-0390
type: task
title: Facade re-exports and client-decoration WindowOptions
status: backlog
priority: high
parent: BIT-US-0120
milestone: BIT-M-0006
author: mcp
labels: [v2, frameless]
estimate: 1
created: 2026-10-07T09:13:08Z
updated: 2026-10-07T09:13:08Z
---

## Description
Re-export `WindowDecorations`, `Decorations`, `WindowControlArea`, `ResizeEdge`, `TitlebarOptions` through `crate::ui`; change window creation in `crates/bitacora-app/src/app.rs` (~line 264) to client decorations, transparent titlebar, `app_owns_titlebar_drag` on macOS.

## Acceptance Criteria
- Window opens without OS title bar on Linux; facade test passes.
