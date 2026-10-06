---
id: BIT-T-0214
type: task
title: Joda-style date pattern formatter and strict parser
status: done
priority: critical
parent: BIT-US-0091
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat, journals]
estimate: 5
created: 2026-10-06T14:31:07Z
updated: 2026-10-06T16:58:59Z
closed: 2026-10-06T16:58:59Z
---

## Description
`crates/bitacora-core/src/journal/datefmt.rs`: compile a cljs-time/Joda pattern into tokens and provide `format(date, pattern)` and `parse(text, pattern) -> Option<NaiveDate>` (use `chrono::NaiveDate` / `jiff::civil::Date` for arithmetic only).

Supported tokens (all patterns listed in Logseq's config template `config.edn` `:journal/page-title-format` comment and the date picker list in `src/main/frontend/date.cljs`): `yyyy`, `yy`, `MMMM`, `MMM`, `MM`, `M`, `dd`, `d`, `do` (ordinal: 1st 2nd 3rd 4th 11th 12th 13th 21st 22nd 23rd 31st), `EEEE`, `EEE`, `E`, quoted literals `'…'`, and punctuation/space literals. English month/day names only (as Logseq).

Parser is strict on literal text but case-insensitive on month/day names; rejects invalid dates (`Feb 30th`).

## Acceptance Criteria
- Round-trip tests format→parse for every date of 2024 and every pattern in Logseq's list (`MMM do, yyyy`, `yyyy-MM-dd`, `yyyy_MM_dd`, `MM/dd/yyyy`, `dd-MM-yyyy`, `yyyy/MM/dd`, `EEEE, dd.MM.yyyy`, `E, MM/dd/yyyy`, `yyyyMMdd`, …).
- Ordinal vectors: 1→`1st`, 2→`2nd`, 3→`3rd`, 11→`11th`, 12→`12th`, 13→`13th`, 22→`22nd`.

## Notes
Refs BIT-SP-0002.R10. Check `../logseq` tag 0.10.15 for the exact pattern list and cite `path:line` in the PR.
