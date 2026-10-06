---
id: BIT-T-0092
type: task
title: Property value interpretation (refs, comma split, typed scalars)
status: done
parent: BIT-US-0058
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, parser, properties]
estimate: 3
created: 2026-10-06T14:29:17Z
updated: 2026-10-06T16:59:58Z
started: 2026-10-06T16:46:41Z
closed: 2026-10-06T16:59:58Z
---

## Description
Implement `crates/bitacora-markdown/src/properties/value.rs`: `fn interpret(key_norm, value_raw, cfg: &PropertyConfig) -> PropValue` where `PropValue = Raw(String) | Quoted(String) | Pages(BTreeSet<String>, Option<String> /*rest text*/) | Bool(bool) | Int(i64) | Str(String)`. Rules in order (`deps/graph-parser/src/logseq/graph_parser/text.cljs:87-187`):
1. keys in the unparsed built-in set (`property.cljs:110-121`) or `cfg.ignored_page_references_keywords` → `Raw`;
2. value wrapped in `"…"` → `Quoted` (quotes kept);
3. refs found by the inline scanner (`[[x]]`, `#tag`, `#[[a b]]`, nested; macros skipped) → page set;
4. keys `alias`, `aliases`, `tags` + `cfg.separated_by_commas` → split on `,` / `，`, plain fragments become pages (`text.cljs:132-163`);
5. `true`/`false` → Bool; `^\d+$` → Int;
6. built-in typed keys (`property.cljs:81-103`) coerce to bool/int.
`PropertyConfig` is a plain struct (filled by bitacora-config later; no crate dependency).

## Acceptance Criteria
- Our own value-interpretation vectors covering the documented cases of [[02-markdown-block-syntax]], verified black-box against Logseq (no Logseq test file copied or translated).
- `tags:: foo, bar` → `{"foo","bar"}`; `foo:: a, b` → `Str("a, b")`; `tags:: "foo, bar"` → `Quoted`; `n:: 1000` → Int; `s:: "1000"` → Quoted.
- `tags:: a, [[b c]], #d` → `{"a","b c","d"}`.

## Notes
Part of BIT-US-0058. Implements BIT-SP-0001.R5. Depends on the inline ref scanner (story "Inline scanner") for step 3; stub with a minimal `[[..]]`/`#tag` scanner if that lands later. ADR-015.
