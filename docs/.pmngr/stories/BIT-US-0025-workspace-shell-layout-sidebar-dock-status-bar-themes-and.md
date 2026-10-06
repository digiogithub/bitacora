---
id: BIT-US-0025
type: story
title: "Workspace shell layout: sidebar, dock, status bar, themes and i18n"
status: in_progress
priority: high
parent: BIT-EP-0002
milestone: BIT-M-0001
author: mcp
labels: [ui, bitacora-app]
estimate: 5
created: 2026-10-06T14:27:43Z
updated: 2026-10-06T16:46:40Z
started: 2026-10-06T16:46:40Z
---

## Description
As a user, I want the Bitacora window to have the familiar Logseq-like chrome — a left sidebar, a main page area with an optional right sidebar, a status bar — and to follow my light/dark preference, so that the app feels like an outliner from the first build and later views have a place to live.

## Acceptance Criteria
- Left `Sidebar` with placeholder sections (Journals, All pages, Favorites, Recent, graph switcher) that can be toggled with a keybinding.
- `DockArea` with a center panel and a right panel; layout persisted as JSON in the data dir and restored on restart.
- `StatusBar` with placeholder slots for sync state, MCP server state and index state.
- Theme follows the system appearance by default; user can pick Light/Dark/System and a bundled GPUI Kit theme from a menu; choice persists.
- All visible strings go through `rust-i18n` `t!()` with an `en` locale file.
- Baseline actions and keymap file (`assets/keymaps/default.json`) for ToggleLeftSidebar, ToggleRightSidebar, ToggleTheme, Quit.

## Notes
- [[gpui-and-gpui-kit]] §1.1 (actions, keybindings), §1.3 (theming), §2.2 (Sidebar, Dock, Resizable, StatusBar, Theme).
- [[crate-stack]] §4.1 (`rust-i18n 4.2.4`, `serde_json`), §5.1 (`assets/`). ADR-001.
