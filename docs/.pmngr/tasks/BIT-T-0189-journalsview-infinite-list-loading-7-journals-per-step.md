---
id: BIT-T-0189
type: task
title: JournalsView infinite list loading 7 journals per step
status: done
priority: high
parent: BIT-US-0076
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui, journals]
estimate: 3
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T19:08:15Z
closed: 2026-10-06T19:08:15Z
---

## Description
`crates/bitacora-app/src/views/journals_view.rs`: GPUI `list` of journal sections (title + embedded read-only outline), newest first via `IndexReader::journals(before_day, 7)`; load 7 more near the end. Sections other than today render blocks only when visible (`list` item render on demand, placeholder height estimate). Title click navigates to the page.

## Acceptance Criteria
- `#[gpui::test]`: with 30 journals, initial render has 7 sections; scrolling loads the next 7.
- Journals with no blocks show an empty placeholder line.

## Notes
[[04-editor-outliner-operations]] §9 (`load-more-journals!` 7 at a time).
