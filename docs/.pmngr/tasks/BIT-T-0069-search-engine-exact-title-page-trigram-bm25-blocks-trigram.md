---
id: BIT-T-0069
type: task
title: "Search engine: exact title, page trigram, bm25 blocks, trigram/LIKE and scopes"
status: backlog
priority: high
parent: BIT-US-0009
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, search]
estimate: 3
created: 2026-10-06T14:28:35Z
updated: 2026-10-06T14:28:35Z
---

## Description
`crates/bitacora-index/src/search/engine.rs`: `search(q, opts{limit, scope: All|Page(id)|Journals|Pages, include_blocks, include_pages}) -> SearchResults`. Runs, on a read connection: exact `pages.name`/alias match; `pages_fts MATCH` (>= 3 chars); `SELECT rowid, bm25(blocks_fts) ... ORDER BY score LIMIT k`; `blocks_fts_tri MATCH` (>= 3 chars) or `LIKE` on `search_text`; scope joins on `blocks.page_id` / `pages.is_journal`. Each source returns a ranked list for fusion. Property name/value suggestions via `SELECT DISTINCT key FROM block_properties` and values by key (for autocomplete).

## Acceptance Criteria
- Tests matching BIT-SP-0003.R14 scenarios (exact title first, FTS syntax neutralized, CJK short query via LIKE).
- Scope `Page(id)` returns only blocks of that page.

## Notes
BIT-SP-0003.R14, BIT-SP-0003.R15. [[sqlite-index-schema]] §6.2.
