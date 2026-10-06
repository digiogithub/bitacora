---
id: BIT-T-0066
type: task
title: Unlinked references via FTS phrase prefilter and Logseq regex
status: done
priority: medium
parent: BIT-US-0008
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 2
created: 2026-10-06T14:28:35Z
updated: 2026-10-06T18:39:52Z
closed: 2026-10-06T18:39:52Z
---

## Description
`crates/bitacora-index/src/read/unlinked.rs`: build `"name" OR "alias1" OR ...` (normalized, quoted) for `blocks_fts MATCH`, exclude blocks on the page, then in Rust strip the `:LOGBOOK:` drawer and apply `(?i)(^|[^\[#0-9a-zA-Z]|((^|[^\[])\[))NAME($|[^0-9a-zA-Z])` (escaped name; `regex` crate) on `content`; return grouped like linked refs. Lazy: only called when the UI expands the section.

## Acceptance Criteria
- Tests matching BIT-SP-0003.R17 scenarios (`call Ana tomorrow` yes; `[[Ana]]` no; `Anaconda` no).
- Never scans `blocks` without an FTS prefilter (EXPLAIN QUERY PLAN test).

## Notes
BIT-SP-0003.R17. Logseq `model.cljs:1320-1350`.
