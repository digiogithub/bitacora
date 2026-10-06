---
id: BIT-US-0102
type: story
title: Live query results rendering as list or table
status: done
priority: high
parent: BIT-EP-0013
milestone: BIT-M-0005
author: mcp
labels: [query, ui, bitacora-app]
estimate: 5
created: 2026-10-06T14:31:46Z
updated: 2026-10-06T22:20:30Z
closed: 2026-10-06T22:20:30Z
---

## Description
As a user, I want `{{query}}` and advanced query blocks to render their results inline (grouped blocks or a table) and update when my notes change, so that I can build dashboards.

## Acceptance Criteria
- Block results grouped by page with breadcrumbs; page results as list.
- Table view honours `query-table::`, `query-properties::`, `query-sort-by::`, `query-sort-desc::` and a column picker.
- Results refresh on relevant `IndexEvent`s (debounced 300 ms) without re-running unrelated queries.
- Query errors and "unsupported" constructs shown inline without breaking the page.

## Notes
Implements: BIT-SP-0003.R18, BIT-SP-0003.R19. [[gpui-and-gpui-kit]] §2.3 (Query blocks: DataTable / List); [[04-editor-outliner-operations]] §8.
