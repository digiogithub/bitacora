---
id: BIT-T-0397
type: task
title: Sidebar nav, favorites/recents and status footer
status: in_review
priority: high
parent: BIT-US-0123
milestone: BIT-M-0006
author: mcp
labels: [v2, ui]
estimate: 3
created: 2026-10-07T09:13:45Z
updated: 2026-10-07T11:21:14Z
started: 2026-10-07T11:21:09Z
---

## Description
Rebuild the sidebar at 252px: nav items (Journals, All pages, Tasks + overdue badge, Graph), Favorites (config.edn `:favorites`) and Recents, footer with graph switcher, sync state and MCP/Pando status dots.

## Acceptance Criteria
- Favorites edits still go through config writes; footer reacts to runtime status events (test).
