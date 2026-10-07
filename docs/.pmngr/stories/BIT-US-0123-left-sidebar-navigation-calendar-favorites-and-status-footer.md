---
id: BIT-US-0123
type: story
title: "Left sidebar: navigation, calendar, favorites and status footer"
status: backlog
priority: high
parent: BIT-EP-0017
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, bitacora-app, bitacora-index]
estimate: 8
created: 2026-10-07T09:13:09Z
updated: 2026-10-07T09:13:09Z
---

## Description
As a user, I want the 252px sidebar of the design: nav (Journals, All pages, Tasks with overdue count, Graph), a month calendar with note dots, Favorites/Recents, and a footer with graph name, sync and MCP/Pando status.

## Acceptance Criteria
- Calendar: Monday-first, dots for days with journal content, today/selected styles, click opens that journal (creating per existing rules only on edit).
- Overdue count from the index task query; footer updates from runtime events.
- Index queries for "days with notes in month" and overdue count added with tests if missing.

## Notes
Design `docs/componentes.md` (calendar), `docs/layout.md`. Current sidebar in `crates/bitacora-app/src/views/`.
