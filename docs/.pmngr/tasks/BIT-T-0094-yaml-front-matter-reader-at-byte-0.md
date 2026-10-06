---
id: BIT-T-0094
type: task
title: YAML front matter reader at byte 0
status: backlog
parent: BIT-US-0059
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, parser, properties]
estimate: 2
created: 2026-10-06T14:29:17Z
updated: 2026-10-06T14:29:17Z
---

## Description
Implement `crates/bitacora-markdown/src/front_matter.rs`: detect `---\n` (or `---\r\n`, optionally after a BOM) at byte 0, read `key: value` lines until the closing `---` line (mldoc `lib/syntax/markdown_front_matter.ml`), and return `FrontMatter { span, entries: Vec<(key_span, key, value_span, value)> }`. Values are taken as trimmed strings (simple YAML subset: scalars and comma lists; anything else kept raw). The front matter stays inside the pre-block's raw bytes. `---` that is not at byte 0 is a horizontal rule, not front matter.

## Acceptance Criteria
- `---\ntitle: Front\ntags: a\n---\n- block` → entries `title=Front`, `tags=a`; pre-block bytes unchanged.
- `- a\n---\n` does not produce front matter.
- CRLF front matter parses; BOM-prefixed front matter parses (BOM excluded from first key).
- Unclosed front matter → treated as plain pre-block text (documented).

## Notes
Part of BIT-US-0059. Implements BIT-SP-0001.R3, R19. See [[01-file-graph-layout]] §8.
