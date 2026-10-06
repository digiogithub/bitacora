---
id: BIT-T-0071
type: task
title: Snippet builder with highlight ranges
status: backlog
priority: medium
parent: BIT-US-0009
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, search]
estimate: 2
created: 2026-10-06T14:28:35Z
updated: 2026-10-06T14:28:35Z
---

## Description
`crates/bitacora-index/src/search/snippet.rs`: from `content` (hidden properties removed) find matches of normalized query terms using an accent/case-folding char map back to original offsets; return `Snippet { text, highlights: Vec<Range<usize>> }` with a ~160-char window around the densest match cluster, ellipses at cut points, char-boundary safe. Used by the app's search palette and MCP `search` tool.

## Acceptance Criteria
- Tests: accented original `Reunión` highlighted for query `reunion`; CJK text; multi-term highlight; window never splits a UTF-8 char.

## Notes
BIT-SP-0003.R14. [[sqlite-index-schema]] §6.2 step 7.
