---
id: BIT-US-0079
type: story
title: "Left sidebar navigation: journals, all pages, favorites and recent"
status: in_progress
priority: medium
parent: BIT-EP-0006
milestone: BIT-M-0002
author: mcp
labels: [ui, bitacora-app]
estimate: 5
created: 2026-10-06T14:29:26Z
updated: 2026-10-06T19:14:39Z
started: 2026-10-06T19:14:39Z
---

## Description
As a reader, I want a left sidebar with Journals, All pages, Favorites and Recent, so that I can reach any page in one or two clicks.

## Acceptance Criteria
- Sidebar entries fed from the index; Favorites read from `:favorites` in `config.edn`; Recent kept per graph in app settings (last 20).
- "All pages" view uses GPUI Kit `DataTable`: name, backlinks count, created/updated, journal flag; sortable, filterable, hides journals and built-ins by default.
- Sidebar collapses with `t l`; selection state reflects the current page.

## Notes
Builds on BIT-US-0025 (layout). [[gpui-and-gpui-kit]] §2.2 (Sidebar, DataTable, VirtualList).
