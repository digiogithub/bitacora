---
id: BIT-US-0101
type: story
title: Simple query DSL compiled to SQL
status: in_progress
priority: high
parent: BIT-EP-0013
milestone: BIT-M-0005
author: mcp
labels: [query, bitacora-index]
estimate: 8
created: 2026-10-06T14:31:46Z
updated: 2026-10-06T18:49:11Z
started: 2026-10-06T18:49:11Z
---

## Description
As a Logseq user, I want my existing `{{query ...}}` blocks to return the same results in Bitacora, so that my dashboards and task lists keep working.

## Acceptance Criteria
- Parser covers the full DSL: `[[x]]`, `#x`, `"text"`, `task`, `priority`, `property` (1/2 args), `page-property`, `between` (journal and timestamp forms), `page`, `namespace`, `page-tags`, `all-page-tags`, `sort-by`, `sample`, `and`/`or`/`not`, with date keywords.
- Result type rule and NULL-safe `not` match Logseq; the query's own block is excluded.
- Every example in [[sqlite-index-schema]] §7.3 and a conformance suite of ≥ 40 cases produce the same results as Logseq on fixtures.
- Deviations listed in §7.4 are documented in user docs.

## Notes
Implements: BIT-SP-0003.R18. [[sqlite-index-schema]] §7; [[03-parsing-indexing-search]] §8.1.
