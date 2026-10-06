---
id: BIT-T-0023
type: task
title: Extract page and block refs with Logseq kinds
status: backlog
priority: critical
parent: BIT-US-0005
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 3
created: 2026-10-06T14:26:33Z
updated: 2026-10-06T14:26:33Z
---

## Description
`crates/bitacora-index/src/derive/refs.rs`: from the inline scanner output of `bitacora-markdown`, emit `(page_name, kind)` per block: 1 `[[x]]`/nested links, 2 `#x`/`#[[x]]`, 3 ref-valued property values, 4 property names (skip when `:property-pages/enabled? false` or key in `:property-pages/excludelist`; skip hidden built-ins), 5 marker page (`TODO`, `DOING`, ...), 6 priority page (`A`/`B`/`C`), 7 namespace parents of every referenced `a/b/c` page (`a`, `a/b`), 8 `{{embed [[x]]}}`. Honour `:ignored-page-references-keywords`. Block refs: `((uuid))` kind 1, `{{embed ((uuid))}}` kind 2, `[label](((uuid)))` kind 3. Collect `referenced_pages` (all names to upsert, original casing).

## Acceptance Criteria
- Test vectors: `TODO [#A] review [[work/q3]]` → `todo`(5), `a`(6), `work/q3`(1), `work`(7); `type:: [[book]], #fiction` → `type`(4), `book`(3), `fiction`(3).
- Refs inside inline code and code fences are ignored (same as Logseq).

## Notes
BIT-SP-0003.R8. [[03-parsing-indexing-search]] §4.1–4.4; [[02-markdown-block-syntax]].
