---
id: BIT-T-0246
type: task
title: Image metadata EDN map parse/print (pr-str style)
status: in_progress
parent: BIT-US-0092
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, serializer]
estimate: 2
created: 2026-10-06T14:32:01Z
updated: 2026-10-06T17:02:01Z
started: 2026-10-06T17:02:01Z
---

## Description
Add `crates/bitacora-markdown/src/inline/image_meta.rs`: parse the `{…}` EDN map that follows `![alt](url)` (e.g. `{:height 100, :width 200}`) into an ordered key/value list; provide `set_size(w, h)` that rewrites only that map in Logseq's `pr-str` style (`editor.cljs:1836-1844`): `{:height 150, :width 300}` — keys as keywords, `, ` separator, insertion order as Logseq produces (`:height` then `:width` for new maps; existing order preserved). Unknown keys are kept. Opaque Markdown constructs (lists, tables, quotes, HTML, hiccup, `#+BEGIN_*`) are never touched by this or any writer function.

## Acceptance Criteria
- Fixture 19 of §11: `![a.png](../assets/a.png){:height 100, :width 200}` round-trips; resize to 300×150 → `{:height 150, :width 300}` and nothing else in the block changes.
- Map with an extra key `{:height 1, :width 2, :x "y"}` keeps `:x "y"`.

## Notes
Part of BIT-US-0092. Implements BIT-SP-0001.R18.
