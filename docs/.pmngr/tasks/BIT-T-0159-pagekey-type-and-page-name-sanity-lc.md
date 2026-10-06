---
id: BIT-T-0159
type: task
title: PageKey type and page_name_sanity_lc
status: backlog
priority: high
parent: BIT-US-0088
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat]
estimate: 2
created: 2026-10-06T14:30:36Z
updated: 2026-10-06T14:30:36Z
---

## Description
`crates/bitacora-core/src/model/page_key.rs`: `PageKey(String)` built only via `PageKey::from_title(&str)`, implementing `page-name-sanity-lc` (`graph_parser/util.cljs:134-142,162-165`): strip one leading and one trailing `/`, NFC, Unicode lower-case (JS `toLowerCase` semantics — use `str::to_lowercase`, document differences such as final sigma). Implement `Hash/Eq/Ord`, `Display`. Used as the page map key everywhere (index, refs, rename).

## Acceptance Criteria
- `Foo`, `foo`, `/foo/` → same key; NFD `Cafe\u{301}` and NFC `Café` → same key.
- `//a//` strips only one slash each side → `/a/`.
- Benchmarks not required; no allocation beyond one String.

## Notes
Refs BIT-SP-0002.R8.
