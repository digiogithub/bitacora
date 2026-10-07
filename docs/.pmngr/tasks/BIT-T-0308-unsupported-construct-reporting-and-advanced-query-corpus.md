---
id: BIT-T-0308
type: task
title: Unsupported-construct reporting and advanced query corpus test
status: done
priority: medium
parent: BIT-US-0103
milestone: BIT-M-0005
author: mcp
labels: [bitacora-index, query, testing]
estimate: 2
created: 2026-10-06T14:33:07Z
updated: 2026-10-07T08:21:12Z
started: 2026-10-06T19:05:50Z
closed: 2026-10-07T08:21:12Z
---

## Description
Return `QueryOutcome { rows, warnings: Vec<Unsupported { construct, span }> }`: `:result-transform`, `:view`, unknown fns/rules, nested reverse-ref pull patterns are reported as `unsupported: <construct>` and the raw rows are still returned when the core `:query` compiled. Build a corpus (`fixtures/queries/advanced/`, ≥ 25 queries written by us to cover the patterns seen in Logseq docs and public graphs; include a query verbatim only if its source license allows redistribution) with expected classification and, for supported ones, expected results exported from Logseq (black-box oracle). Record findings in [[sqlite-index-schema]] open question 7.

## Acceptance Criteria
- Corpus test passes; the supported share is reported in test output.
- BIT-SP-0003.R19 scenario "Unsupported view" passes.

## Notes
BIT-SP-0003.R19. ADR-015.
