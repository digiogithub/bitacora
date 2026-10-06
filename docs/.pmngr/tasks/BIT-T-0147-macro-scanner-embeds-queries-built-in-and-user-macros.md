---
id: BIT-T-0147
type: task
title: "Macro scanner: embeds, queries, built-in and user macros"
status: backlog
parent: BIT-US-0083
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, parser, refs]
estimate: 2
created: 2026-10-06T14:30:15Z
updated: 2026-10-06T14:30:15Z
---

## Description
Add `crates/bitacora-markdown/src/inline/macros.rs`: `{{name args}}` and `{{{name args}}}` per `mldoc lib/syntax/inline.ml:1047-1090`. `name` runs until space, `(` or `}`; arguments are comma-separated, each being a `[[…]]` ref, `((…))` ref, `"quoted string"` (commas allowed) or plain text up to `,`. Macros never span lines. Emit `Macro { name, args: Vec<MacroArg>, span }`. Derive refs: `embed` with `[[page]]` → page ref (`block.cljs:70-75`), with `((uuid))` → block ref (`block.cljs:99-105`); `query` arguments are scanned for refs but the macro is captured for the query engine. User macros from `:macros` are **never** expanded into text (render-time only).

## Acceptance Criteria
- Fixture 14 of §11: every built-in macro name (`query`, `function`, `namespace`, `youtube`, `youtube-timestamp`, `zotero-imported-file`, `zotero-linked-file`, `vimeo`, `bilibili`, `video`, `tweet`, `twitter`, `embed`, `renderer`, `cloze`, `cards`, `img`) plus a user macro parse with correct args.
- `{{embed ((6500c1a4-0000-4000-8000-000000000001))}}` → block ref.
- `{{cloze "a, b"}}` → one argument `a, b`.

## Notes
Part of BIT-US-0083. Implements BIT-SP-0001.R8.
