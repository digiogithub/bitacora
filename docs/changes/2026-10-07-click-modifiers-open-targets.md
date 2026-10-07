---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - app
    - navigation
---
# Consistent click modifiers: plain, Shift (sidebar), Ctrl/Cmd (new tab)

Continues [[bitacora-v2-plan]]. Backlog: story under BIT-EP-0017.

## What
- `nav::OpenIn` gained `NewTab` and the single mapping `OpenIn::from_flags` / `from_modifiers` (Shift wins, then the platform `secondary` key). Every surface that handled Shift now calls it: inline refs/tags/block refs and bullets (`block_view`), page and journal views, journal titles (`JournalsView::open_entry_in`), All pages, query and embed blocks, Tasks rows, graph nodes, sidebar favorites/recents (`LeftSidebar::click_page`), palette results.
- Events: `PageEvent::OpenInNewTab`, `MainEvent::OpenInNewTab`, `SidebarEvent::OpenInNewTab`, `StackEvent::OpenInNewTab`, `PaletteEvent::Open { open: OpenIn }` (was `sidebar: bool`).
- `Workspace::open_in_new_tab` pushes `TabStrip::open(route)` then navigates; the previous tab keeps its route.
- Graph view: focus toggle moved from Ctrl/Cmd+click to Alt+click so Ctrl/Cmd+click can open a tab.

## Verification
Unit tests in `nav.rs`, gpui tests in sidebar (real clicks with modifiers), graph, all_pages, journals, workspace (tab strip result).
