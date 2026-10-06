---
id: BIT-T-0302
type: task
title: "QueryBlock view: grouped block results and page lists"
status: backlog
priority: high
parent: BIT-US-0102
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, query, ui]
estimate: 3
created: 2026-10-06T14:33:07Z
updated: 2026-10-06T14:33:07Z
---

## Description
`crates/bitacora-app/src/render/query_block.rs`: replace the `{{query}}` placeholder with a `QueryBlock` entity: header (query title or text, result count, refresh, collapse), runs `bitacora_index::query::execute` on a background executor, renders block results grouped by page with breadcrumbs (reusing `BlockView`) or page results as a `List`; error/unsupported shown as GPUI Kit `Alert` inline. Honour `collapsed?`.

## Acceptance Criteria
- `#[gpui::test]`: a fixture page with three queries renders all with correct counts; a malformed query shows an Alert and the rest of the page renders.

## Notes
BIT-SP-0003.R18.
