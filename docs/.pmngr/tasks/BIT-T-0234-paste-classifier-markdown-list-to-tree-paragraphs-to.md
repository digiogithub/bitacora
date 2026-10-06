---
id: BIT-T-0234
type: task
title: "Paste classifier: Markdown list to tree, paragraphs to siblings, inline otherwise"
status: backlog
priority: high
parent: BIT-US-0037
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, clipboard]
estimate: 3
created: 2026-10-06T14:31:34Z
updated: 2026-10-06T14:31:34Z
---

## Description
`crates/bitacora-core/src/paste.rs`: `classify_paste(text) -> PasteKind::{Blocks(Vec<FlatBlock>), Paragraphs(Vec<String>), Inline(String)}`. Normalise CRLF; if any line matches `^\s*([-+*]|#+)\s+` parse with the `bitacora-markdown` outline parser into a flat list with levels (normalise mixed indentation relative to the minimum); blank-line separated → paragraphs; else inline. Private payload takes precedence when present.

## Acceptance Criteria
- Table tests: nested `-` list with 2-space indent, `*` bullets, `#` headings, code fence containing `- ` (stays one block), paragraphs, single line.
- Fuzz test: never panics on arbitrary UTF-8.

## Notes
Story BIT-US-0037. Implements BIT-SP-0004.R13. Logseq `paste.cljs:23-47`, `:101`.
