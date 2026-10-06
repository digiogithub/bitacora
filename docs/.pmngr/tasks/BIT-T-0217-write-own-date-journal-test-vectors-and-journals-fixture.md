---
id: BIT-T-0217
type: task
title: Write own date/journal test vectors and journals fixture graph
status: done
priority: medium
parent: BIT-US-0091
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, test, compat, journals]
estimate: 2
created: 2026-10-06T14:31:08Z
updated: 2026-10-06T16:58:59Z
closed: 2026-10-06T16:58:59Z
---

## Description
Write our own journal/date test vectors in `crates/bitacora-core/tests/journals.rs` covering the documented cases of BIT-SP-0002.R10/R11 and [[01-file-graph-layout]] (title formats, file-name formats, journal detection, `journal-day` values, invalid dates), with expected results verified black-box against Logseq 0.10.15. Do not copy or translate Logseq's date/journal test files. Add a fixture graph `fixtures/graphs/journals/` with journals in `journals/` and `pages/`, a `title:: Nov 14th, 2025` page, and a config using `:journal/page-title-format "EEEE, dd.MM.yyyy"`.

## Acceptance Criteria
- All our vectors pass; fixture graph yields the expected journal set and `journal-day` values (golden JSON).

## Notes
Refs BIT-SP-0002.R10, BIT-SP-0002.R11. ADR-015 (own vectors; Logseq only as black-box oracle).
