---
id: BIT-T-0129
type: task
title: Rewrite property keys and property-value refs on rename
status: done
parent: BIT-US-0082
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, rename]
estimate: 2
created: 2026-10-06T14:29:56Z
updated: 2026-10-06T19:30:10Z
closed: 2026-10-06T19:30:10Z
---

## Description
Extend `ref_rewrite.rs`: property key `old::` (key equals old page key) → `new-key::` where new key = lower-case new name with spaces → `-` (`page.cljs:272-278`). Property values: rewrite `[[Old]]`/`#Old` and, for comma-separated keys (`tags`, `alias`, `aliases`, `:property/separated-by-commas`), plain fragments equal to the old name (`:289-311`). Quoted values (`"…"`) are not touched. Only the affected property line changes.

## Acceptance Criteria
- `old:: value` → `new-idea:: value`.
- `tags:: Old, x` → `tags:: New Idea, x`; `foo:: Old` (non-comma key, plain text) unchanged; `foo:: [[Old]]` → `foo:: [[New Idea]]`.
- `title:: "Old"` unchanged.
- Expected outputs cross-checked against Logseq 0.10.15 for each case (record source in test comments).

## Notes
BIT-SP-0002.R13; BIT-SP-0001.R5.
