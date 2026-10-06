---
id: BIT-T-0245
type: task
title: Canonical block writer (transform-content behaviour without quirks)
status: done
parent: BIT-US-0092
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, serializer]
estimate: 3
created: 2026-10-06T14:32:01Z
updated: 2026-10-06T17:08:48Z
started: 2026-10-06T17:02:01Z
closed: 2026-10-06T17:08:48Z
---

## Description
Implement `crates/bitacora-markdown/src/canonical.rs`: `fn write_block(content: &str, depth: usize, unit: IndentUnit, opts) -> String`, re-implementing the canonical block serialisation documented in [[02-markdown-block-syntax]] §7 (Logseq reference for behaviour only: `transform-content`, `src/main/frontend/modules/file/core.cljs:34-90`):
- `IndentUnit` from `:export/bullet-indentation`: `:tab` (default `\t`), `:two-spaces`, `:four-spaces`, `:eight-spaces` (`state.cljs:553-563`);
- prefix `unit*(depth-1) + "-"`, separator `" "` unless body blank, continuation `unit*(depth-1) + "  "` for every following line (blank inner lines become the bare continuation prefix, `core.cljs:15-18`);
- content trimmed at edges, split on `\r?\n`, LF output;
- empty block → `-`;
- pre-block → `trim(content) + "\n"` followed by the join `"\n"` (one blank line before first block, `core.cljs:45-48`);
- deliberately NOT reproduced: bullet-less first block for `heading:: true` (§7 item 3) and auto pre-block conversion of a first block starting with `key:: ` (§7 item 2) — first blocks always keep `- `;
- Markdown `:PROPERTIES:` drawer of an edited block converted to `key:: value` lines (`property.cljs:135-158`).

## Acceptance Criteria
- Golden test: canonical example of [[02-markdown-block-syntax]] §7 rebuilt from a model produces byte-identical output.
- `:two-spaces` depth 3 block `x` → `"    - x"`.
- New page with `tags:: demo` + empty block → `"tags:: demo\n\n-"`.
- Edited `heading:: true` first block → `"- Intro\n  heading:: true"`.

## Notes
Part of BIT-US-0092. Implements BIT-SP-0001.R11, R14, R17. ADR-015 (re-implemented from docs, no Logseq code copied/translated).
