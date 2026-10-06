---
id: BIT-T-0303
type: task
title: Query table view with query-table/query-properties/query-sort-by
status: backlog
priority: medium
parent: BIT-US-0102
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, query, ui]
estimate: 3
created: 2026-10-06T14:33:07Z
updated: 2026-10-06T14:33:07Z
---

## Description
Table mode for `QueryBlock` using GPUI Kit `DataTable`: columns `block`/`page` plus selected properties; read `query-table::`, `query-properties::`, `query-sort-by::`, `query-sort-desc::` from the query block; toggle list/table and column picker write those properties back through the core command queue (one undoable transaction). Sorting by column clicks numeric-aware.

## Acceptance Criteria
- `#[gpui::test]`: table shows property columns from `query-properties:: [:type :rating]`; changing sort updates `query-sort-by::` in the file (byte-minimal diff).

## Notes
BIT-SP-0003.R18. [[sqlite-index-schema]] §7.1 step 5.
