---
id: BIT-T-0024
type: task
title: Type property values into EAV rows (refs, int, bool, string)
status: backlog
priority: high
parent: BIT-US-0005
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 3
created: 2026-10-06T14:26:33Z
updated: 2026-10-06T14:26:33Z
---

## Description
`crates/bitacora-index/src/derive/props.rs`: for each block property produce a `block_properties` row (normalized key: lower-case, `_`/space → `-`; `pos`; `raw_key`; `raw_value`; `value_type` 0 string / 1 integer / 2 boolean / 3 refs; `builtin` 0/1/2 from the built-in list) and `block_property_values` rows (`value_norm` lower+NFC or `page-name-sanity-lc` for refs, `value_num` for int/bool, `ref_page_name`). Rules: `[[x]]`/`#x` → refs; keys in `:property/separated-by-commas` (plus `alias`, `tags`) split on commas into refs; quoted strings verbatim; `created-at`/`updated-at` integers also fill block columns. Invalid keys → `invalid_property` diagnostic.

## Acceptance Criteria
- Tests: `rating:: 4`, `public:: true`, `authors:: Ana, Ben` with comma config, `title:: "Hello, World"` (raw kept byte for byte).
- Page `alias::` and `tags::` on the pre-block produce `PageDef.aliases` / `tags`.

## Notes
BIT-SP-0003.R10, BIT-SP-0003.R11. [[03-parsing-indexing-search]] §3.4.
