---
id: BIT-T-0301
type: task
title: Query DSL conformance suite against Logseq results
status: in_progress
priority: high
parent: BIT-US-0101
milestone: BIT-M-0005
author: mcp
labels: [bitacora-index, query, testing]
estimate: 3
created: 2026-10-06T14:33:07Z
updated: 2026-10-06T18:49:11Z
started: 2026-10-06T18:49:11Z
---

## Description
`fixtures/graphs/queries/` graph plus `crates/bitacora-index/tests/query_dsl_conformance.rs`: ≥ 40 query cases (each leaf, combinations, `not` on tasks, aliases not expanded by `[[x]]`, `between` journal and created-at, `sort-by`, page vs block result type) with expected block/page lists exported from Logseq 0.10.15 (script documented in `fixtures/README.md`). Deviations from §7.4 (case-insensitive text/property) are marked as expected differences with a reason.

## Acceptance Criteria
- All cases pass in CI; each expected-difference case links to §7.4 entry.
- `query.execute()` p95 < 50 ms per case on the medium generated graph (bench in same file, ignored by default).

## Notes
BIT-SP-0003.R18. Epic AC of BIT-EP-0013.
