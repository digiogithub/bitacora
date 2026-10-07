---
id: BIT-T-0395
type: task
title: Top bar buttons and search field wired to existing actions
status: backlog
priority: high
parent: BIT-US-0122
milestone: BIT-M-0006
author: mcp
labels: [v2, ui]
estimate: 3
created: 2026-10-07T09:13:45Z
updated: 2026-10-07T09:13:45Z
---

## Description
Sidebar toggle, back/forward (history), search field with ⌘K/Ctrl+K hint opening the palette, theme toggle, PDF export, AI/Pando button (placeholder action), right-panel toggle, all inside `AppTitleBar` and outside the drag area.

## Acceptance Criteria
- `#[gpui::test]` dispatches each action; keymap tests unchanged.
