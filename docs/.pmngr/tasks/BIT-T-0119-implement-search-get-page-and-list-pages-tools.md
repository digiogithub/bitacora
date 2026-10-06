---
id: BIT-T-0119
type: task
title: Implement search, get_page and list_pages tools
status: in_progress
priority: high
parent: BIT-US-0017
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, tools, read]
estimate: 3
created: 2026-10-06T14:29:55Z
updated: 2026-10-06T18:44:25Z
started: 2026-10-06T18:44:25Z
---

## Description
`crates/bitacora-mcp/src/tools/read_pages.rs`: `search {query, limit=20, kind?, page?}` over the index FTS5 (word + trigram) returning `{type, uuid?, page, snippet, score, breadcrumb}`; `get_page {name}` resolving case-insensitive name/alias via the index → `{name, original_name, uuid, properties, aliases, journal_day?, file, updated_at, block_count, etag}`; `list_pages {namespace?, tag?, modified_since?, limit, cursor}`. Descriptions mention `logseq.search`, `Editor.getPage`, `Editor.getAllPages`/`getPagesFromNamespace`. `readOnlyHint: true`.

## Acceptance Criteria
- Tool tests on `fixtures/graphs/*`: alias lookup, `NOT_FOUND`, namespace filter, pagination of 100.

## Notes
Story BIT-US-0017. Implements BIT-SP-0007.R11. See [[sqlite-index-schema]].
