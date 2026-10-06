---
id: BIT-US-0009
type: story
title: Full-text search with ranking, fuzzy titles and snippets
status: backlog
priority: high
parent: BIT-EP-0005
milestone: BIT-M-0002
author: mcp
labels: [index, search, bitacora-index]
estimate: 8
created: 2026-10-06T14:25:24Z
updated: 2026-10-06T14:25:24Z
---

## Description
As a user, I want to type a few words and instantly get the best matching pages and blocks, with accents, case and CJK handled, so that I can find anything in my graph.

## Acceptance Criteria
- Query parser turns `and`/`or`/`not` into FTS5 operators and quotes everything else; punctuation never produces an FTS error.
- Ranking: exact title/alias → title trigram + `nucleo` fuzzy → bm25 blocks → trigram/LIKE → RRF (k = 60).
- Snippets with highlight ranges built in Rust from `content`.
- Scopes: current page, journals only, pages only.
- `search.substring = false` drops `blocks_fts_tri` and falls back to `LIKE`.
- p95 < 50 ms on the 5,000-page benchmark graph.

## Notes
Implements: BIT-SP-0003.R13, BIT-SP-0003.R14, BIT-SP-0003.R15. See [[sqlite-index-schema]] §6, [[03-parsing-indexing-search]] §7.
