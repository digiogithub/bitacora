---
id: BIT-T-0216
type: task
title: Journal file path function and never-rename guard
status: done
priority: high
parent: BIT-US-0091
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat, journals]
estimate: 1
created: 2026-10-06T14:31:08Z
updated: 2026-10-06T16:58:59Z
closed: 2026-10-06T16:58:59Z
---

## Description
`crates/bitacora-core/src/journal/path.rs`: `journal_file_path(date, cfg) -> GraphPath` = `<journals_directory>/<format(date, journal_file_name_format | "yyyy_MM_dd")>.md` (`date.cljs:195-211`, `modules/file/core.cljs:124-136`). Add `Page::is_journal()` and a `rename_policy()` returning `KeepFile` for journals, consumed by EP-0009's rename (`page.cljs:486`).

## Acceptance Criteria
- 2025-11-14 default → `journals/2025_11_14.md`; with `daily` + `yyyy-MM-dd` → `daily/2025-11-14.md`.
- Changing `:journal/file-name-format` affects only new paths; existing journal pages keep their `GraphPath`.

## Notes
Refs BIT-SP-0002.R11.
