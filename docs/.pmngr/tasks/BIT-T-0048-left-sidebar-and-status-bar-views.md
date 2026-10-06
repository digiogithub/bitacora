---
id: BIT-T-0048
type: task
title: Left sidebar and status bar views
status: backlog
priority: high
parent: BIT-US-0025
milestone: BIT-M-0001
author: mcp
labels: [ui, bitacora-app]
estimate: 2
created: 2026-10-06T14:28:06Z
updated: 2026-10-06T14:28:06Z
---

## Description
- `src/views/sidebar.rs`: `LeftSidebar` entity using GPUI Kit `Sidebar` with Lucide icons (`gpui-kit-assets`) and placeholder items: Journals, All pages, Favorites (empty), Recent (empty), graph name header. Emits a `SidebarEvent::Navigate(Target)` (targets are stubs for now).
- `src/views/status_bar.rs`: `StatusBar` entity (GPUI Kit `StatusBar`) with three slots: sync state, MCP state, index state (text + icon), each settable via methods; reuse the heartbeat demo slot from the tokio bridge story.
- Wire both into the `Workspace` view; the sidebar collapses/expands with an action.

## Acceptance Criteria
- `#[gpui::test]`: dispatching `ToggleLeftSidebar` toggles visibility state.
- Clicking a sidebar item emits the navigation event (unit-tested via subscription).

## Notes
- [[gpui-and-gpui-kit]] §2.2 (Sidebar, StatusBar, Icon).
