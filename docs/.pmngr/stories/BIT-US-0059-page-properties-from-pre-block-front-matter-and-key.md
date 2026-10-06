---
id: BIT-US-0059
type: story
title: "Page properties from pre-block, front matter and #+key directives"
status: done
priority: high
parent: BIT-EP-0003
milestone: BIT-M-0002
author: mcp
labels: [markdown, compat, parser, properties]
estimate: 5
created: 2026-10-06T14:28:45Z
updated: 2026-10-06T17:23:44Z
closed: 2026-10-06T17:23:44Z
---

## Description
As a Bitacora user, I want page-level properties (`title::`, `alias::`, `tags::`, …) read from the same places Logseq reads them — the property pre-block, YAML front matter at byte 0, and `#+key: value` directives anywhere in the file — so that page titles, aliases and tags resolve identically.

Builds on the outline splitter's pre-block span and the property scanner. Mirrors `extract.cljc:227-241` (page properties from the first AST node) and `collect-page-properties` (`mldoc.cljc:117-131`) which hoists all `Directive` nodes.

## Acceptance Criteria
- `"title:: My Page\nalias:: Mine, [[My page alias]]\ntags:: project, [[multi word]]\n\n- first block"` yields title/alias/tags as in BIT-SP-0001.R3.
- `---\ntitle: Front\ntags: a\n---\n- block` yields `title = Front` with front-matter bytes preserved.
- `- a\n\t- #+title: x` sets page title `x`.
- A first block whose properties contain `heading` is not treated as a pre-block.
- BOM before `title::` does not leak into the key.

## Notes
Implements: BIT-SP-0001.R3, BIT-SP-0001.R19.
See [[02-markdown-block-syntax]] §2.5; [[01-file-graph-layout]] §3.1, §8, §9 (title derivation lives in bitacora-core, BIT-EP-0004).
