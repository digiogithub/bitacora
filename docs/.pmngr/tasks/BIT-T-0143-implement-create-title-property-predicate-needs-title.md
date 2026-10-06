---
id: BIT-T-0143
type: task
title: Implement create-title-property predicate (needs_title_property)
status: backlog
priority: high
parent: BIT-US-0085
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat]
estimate: 1
created: 2026-10-06T14:30:11Z
updated: 2026-10-06T14:38:19Z
---

## Description
`crates/bitacora-core/src/naming/mod.rs`: `needs_title_property(title, format, on_rename: bool) -> bool`, re-implemented from the documented behaviour in [[01-file-graph-layout]] / BIT-SP-0002.R7 (Logseq reference for behaviour only: `fs-util/create-title-property?`, `src/main/frontend/util/fs.cljs:198-206`, as called from `handler/page.cljs:95-99` (create, legacy guard) and `page.cljs:483-484` (rename, no guard)): true when `decode(encode(title)) != page_name_sanity(title)` or the encoded name contains reserved chars.

## Acceptance Criteria
- Create in legacy: `Version 1.0` → true, `My Page` → false, `What? A` → true.
- Create in triple-lowbar → always false.
- Rename path returns the raw round-trip check result regardless of format.

## Notes
Refs BIT-SP-0002.R7. Consumed by EP-0009 lazy page creation and rename. ADR-015 (re-implemented from docs, no Logseq code copied/translated).
