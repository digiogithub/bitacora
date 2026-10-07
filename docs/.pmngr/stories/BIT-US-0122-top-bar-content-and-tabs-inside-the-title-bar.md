---
id: BIT-US-0122
type: story
title: Top bar content and tabs inside the title bar
status: backlog
priority: high
parent: BIT-EP-0017
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, bitacora-app]
estimate: 8
created: 2026-10-07T09:13:09Z
updated: 2026-10-07T09:13:09Z
---

## Description
As a user, I want the top bar of the design (sidebar toggle, back/forward, tabs, search field with ⌘K hint, theme, PDF, Pando/AI and right-panel buttons) so that navigation lives in one place.

## Acceptance Criteria
- Every button wired to the existing actions; shortcuts unchanged; search field opens the existing palette.
- Dock tabs rendered inside the title bar with the design's active-tab style; open/close/reorder/persisted layout keep working; dragging a tab never moves the window.
- A Pando/AI button placeholder opens the Pando settings (wired by the Pando integration epic).

## Notes
Depends on the AppTitleBar and component kit stories. Design: `docs/layout.md` (52px bar), `mockups/Main.dc.html`. Current tabs: `views/workspace.rs` `DockLayout::tabs()`.
