---
id: BIT-T-0299
type: task
title: Date argument resolver for DSL keywords
status: in_progress
priority: high
parent: BIT-US-0101
milestone: BIT-M-0005
author: mcp
labels: [bitacora-index, query]
estimate: 2
created: 2026-10-06T14:33:07Z
updated: 2026-10-06T18:49:11Z
started: 2026-10-06T18:49:11Z
---

## Description
`crates/bitacora-index/src/query/dates.rs`: resolve `today`, `yesterday`, `tomorrow`, `±N(d|w|m|y)`, `[[Journal title]]` (using the configured journal title format via `bitacora-core`) to `yyyyMMdd` ints, and `now`, `±Nh`, `±Nmin` to epoch ms, against an injected clock and the user's local timezone (`query_dsl.cljs:52-113`).

## Acceptance Criteria
- Table tests with a fixed clock (2026-10-06 10:00 local): `-7d` → 20260929, `+1m` → 20261106, `[[Oct 1st, 2026]]` → 20261001, `-2h` → ms.

## Notes
BIT-SP-0003.R18.
