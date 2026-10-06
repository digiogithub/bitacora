---
id: BIT-US-0091
type: story
title: Journal detection, journal-day and journal file naming
status: in_review
priority: critical
parent: BIT-EP-0004
milestone: BIT-M-0002
author: mcp
labels: [core, compat, journals]
estimate: 5
created: 2026-10-06T14:30:44Z
updated: 2026-10-06T16:59:05Z
started: 2026-10-06T16:54:06Z
---

## Description
As a daily-notes user, I want Bitacora to recognise exactly the same journal pages as Logseq, show them with my configured title format, and name new journal files the same way, so that my journals list and `[[Nov 14th, 2025]]` links behave identically.

Needs a Joda/date-fns-compatible formatter/parser for the pattern subset Logseq uses (including the ordinal `do`), the detection pipeline (capitalize-all + ordered formatter list) and the file-name function.

## Acceptance Criteria
- `journals/2025_11_14.md` → journal, title `Nov 14th, 2025`, key `nov 14th, 2025`, journal-day `20251114`.
- `pages/2024_01_01.md` → journal `20240101`.
- `:journal/page-title-format "yyyy-MM-dd"` → title `2025-11-14`; `[[2025-11-14]]` resolves.
- New journal file path: default `journals/2025_11_14.md`; with `:journals-directory "daily"` + `:journal/file-name-format "yyyy-MM-dd"` → `daily/2025-11-14.md`.
- Journal pages are flagged so rename never moves their file.

## Notes
Implements: BIT-SP-0002.R10, BIT-SP-0002.R11
[[01-file-graph-layout]] §4; Logseq `date_time_util.cljs:15-51`, `block.cljs:273-284`, `date.cljs:195-211`. [[architecture]]
