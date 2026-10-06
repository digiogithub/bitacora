---
id: BIT-T-0068
type: task
title: Search query parser producing safe FTS5 MATCH expressions
status: backlog
priority: high
parent: BIT-US-0009
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, search]
estimate: 2
created: 2026-10-06T14:28:35Z
updated: 2026-10-06T14:28:35Z
---

## Description
`crates/bitacora-index/src/search/query.rs`: tokenize user input; map ` and `/`&` → `AND`, ` or `/`|` → `OR`, ` not ` → `NOT`; normalize each other token with `normalize()` and wrap in double quotes (escape `"` as `""`); input with punctuation becomes a phrase. Produce variants: word MATCH for `blocks_fts` (with `*` prefix on the last token), quoted substring for `blocks_fts_tri`/`pages_fts`, `LIKE` pattern with `\` escaping for queries < 3 chars.

## Acceptance Criteria
- Unit tests: `c++ (advanced` produces a valid MATCH; `foo or bar` → `"foo" OR "bar"`; `"quoted"` handled; empty/whitespace query returns no work.
- Fuzz test (`proptest` arbitrary strings) never yields an SQLite FTS syntax error.

## Notes
BIT-SP-0003.R14. [[sqlite-index-schema]] §6.2 step 1.
