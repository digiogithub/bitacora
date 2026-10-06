---
id: BIT-T-0215
type: task
title: Journal detection pipeline and title rendering
status: backlog
priority: critical
parent: BIT-US-0091
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat, journals]
estimate: 2
created: 2026-10-06T14:31:07Z
updated: 2026-10-06T14:39:30Z
---

## Description
`crates/bitacora-core/src/journal/detect.rs`: `detect_journal(title, cfg) -> Option<JournalInfo { day: u32 /*yyyyMMdd*/, title: String }>`, re-implemented from the documented behaviour in [[01-file-graph-layout]] / BIT-SP-0002.R10 (Logseq reference for behaviour only: `convert-page-if-journal`, `block.cljs:273-284`; `valid-journal-title?`/`journal-title->int`, `date_time_util.cljs:15-51`):
1. `capitalize-all` the title (capitalize each whitespace-separated word, rest lower-case).
2. Try formatters in order: `[cfg.journal_page_title_format(), "MMM do, yyyy", "yyyy-MM-dd", "yyyy_MM_dd"]`.
3. On success, `day = yyyyMMdd`, display title = `format(date, cfg.journal_page_title_format())`, key = lower-case of display title.
Detection is by title only (directory irrelevant); runs after `derive_title`.

## Acceptance Criteria
- Scenarios of BIT-SP-0002.R10 pass (default, `pages/2024_01_01.md`, custom `yyyy-MM-dd`).
- `nov 14th, 2025` (lower-case title from `title::`) detected thanks to capitalize-all.
- Custom file format `yyyyMMdd` file not detected unless it matches the page-title format (documented, open question 3 in [[01-file-graph-layout]]).

## Notes
Refs BIT-SP-0002.R10. ADR-015 (re-implemented from docs, no Logseq code copied/translated).
