---
id: BIT-T-0187
type: task
title: "Page header: title, page properties, alias redirect and namespaces"
status: backlog
priority: medium
parent: BIT-US-0075
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui]
estimate: 2
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T14:30:56Z
---

## Description
`crates/bitacora-app/src/views/page_header.rs`: title (original name), page properties (pre-block) via the properties table, alias redirect (empty alias page → source page with "Redirected from X" note), namespace breadcrumb `a / b / c` (each segment clickable) and a "Hierarchy" list of child namespaces (`IndexReader::namespace_children`, GPUI Kit `Tree` for deep trees). Placeholder pages show "No content yet".

## Acceptance Criteria
- `#[gpui::test]` for redirect and namespace breadcrumb navigation.

## Notes
BIT-SP-0003.R9 (aliases). [[03-parsing-indexing-search]] §4.4–4.5.
