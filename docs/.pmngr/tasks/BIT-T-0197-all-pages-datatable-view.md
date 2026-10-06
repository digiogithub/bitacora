---
id: BIT-T-0197
type: task
title: All pages DataTable view
status: in_progress
priority: medium
parent: BIT-US-0079
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui, bitacora-index]
estimate: 3
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T19:14:39Z
started: 2026-10-06T19:14:39Z
---

## Description
`crates/bitacora-app/src/views/all_pages.rs`: GPUI Kit `DataTable` (virtualized) with columns Name, Backlinks, Created, Updated, Type (page/journal/placeholder); sort by any column; filter `Input` (fuzzy on title); toggles "Include journals", "Include placeholders/property pages". Data from `IndexReader::all_pages(filter, sort)` with a backlink count subquery (add to `bitacora-index` if missing).

## Acceptance Criteria
- Renders 5,000 pages without jank; sorting by backlinks returns correct order on fixtures (test on the reader query).
- Clicking a row navigates; Enter on focused row navigates.

## Notes
[[gpui-and-gpui-kit]] §2.2 (DataTable). Open question 6 of [[sqlite-index-schema]] (property pages hidden by default).
