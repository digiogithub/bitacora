---
id: BIT-T-0194
type: task
title: Search palette on GPUI Kit Command with debounced async queries
status: backlog
priority: high
parent: BIT-US-0078
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui, search]
estimate: 3
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T14:30:56Z
---

## Description
`crates/bitacora-app/src/views/search_palette.rs`: Mod+K opens a `Command` dialog with an `Input`; each keystroke (30 ms debounce) spawns a background `search()` and drops stale tasks (generation counter). Sections "Pages" and "Blocks" with snippet highlight runs and page breadcrumb; scope chips (This page / Journals / Pages) toggled with Tab. Enter navigates; Shift+Enter opens in right sidebar; "Create page …" disabled in read-only mode.

## Acceptance Criteria
- `#[gpui::test]`: typing `rust` then Enter navigates to page `Rust`; stale results never replace newer ones.
- Up/Down/Enter/Esc work without the mouse; focus returns to the previous view on close.

## Notes
BIT-SP-0003.R14. [[gpui-and-gpui-kit]] §2.2 (Command).
