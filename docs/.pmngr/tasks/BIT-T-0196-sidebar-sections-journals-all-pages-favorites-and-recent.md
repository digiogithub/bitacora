---
id: BIT-T-0196
type: task
title: "Sidebar sections: journals, all pages, favorites and recent"
status: done
priority: medium
parent: BIT-US-0079
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui]
estimate: 2
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T19:52:50Z
started: 2026-10-06T19:14:39Z
closed: 2026-10-06T19:52:50Z
---

## Description
`crates/bitacora-app/src/views/left_sidebar.rs`: populate the GPUI Kit `Sidebar` (layout from BIT-US-0025) with Journals, All pages, a Favorites group (`:favorites` from `config.edn`, resolved to pages, missing ones shown muted) and Recent (last 20 visited pages per graph, persisted in app settings, updated by `Navigator`). Highlight current route.

## Acceptance Criteria
- `#[gpui::test]`: favorites reflect `config.edn`; visiting pages updates Recent order; list persists across restart.

## Notes
[[gpui-and-gpui-kit]] §2.2 (Sidebar).
