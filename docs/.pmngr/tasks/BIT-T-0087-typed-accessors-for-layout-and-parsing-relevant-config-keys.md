---
id: BIT-T-0087
type: task
title: Typed accessors for layout- and parsing-relevant config keys
status: done
priority: high
parent: BIT-US-0056
milestone: BIT-M-0002
author: mcp
labels: [bitacora-config, compat]
estimate: 2
created: 2026-10-06T14:28:54Z
updated: 2026-10-06T16:52:52Z
started: 2026-10-06T16:46:26Z
closed: 2026-10-06T16:52:52Z
---

## Description
`crates/bitacora-config/src/keys.rs`: typed getters on `EffectiveConfig` with Logseq defaults ([[01-file-graph-layout]] §2.2):
- `name_format() -> NameFormat {Legacy, TripleLowbar}`; `pages_directory()` ("pages"); `journals_directory()` ("journals", also accept misspelled `:journal-directory`? — no: only document it); `whiteboards_directory()`;
- `journal_page_title_format()` (`:journal/page-title-format`, legacy alias `:date-formatter`, default `"MMM do, yyyy"`); `journal_file_name_format()` (`"yyyy_MM_dd"`);
- `preferred_format()`, `hidden() -> Vec<String>`, `default_templates_journals()`, `enable_journals()`, `property_separated_by_commas()`, `ignored_page_references_keywords()`, `block_hidden_properties()`, `bullet_indentation() -> {Tab, TwoSpaces, FourSpaces, EightSpaces}`, `favorites()`, `default_home()`, `preferred_workflow()`, `enable_timetracking()`, `macros()`.
- Invalid types fall back to default + diagnostic.

## Acceptance Criteria
- Unit test per accessor: default, explicit value, wrong-type fallback.
- `:date-formatter "yyyy-MM-dd"` honoured when `:journal/page-title-format` is absent.

## Notes
Refs BIT-SP-0002.R3. These accessors are consumed by bitacora-core (naming, journals) and bitacora-markdown (indent unit, comma-separated keys).
