---
id: BIT-T-0085
type: task
title: EDN reader to value model with duplicate-key detection
status: backlog
priority: critical
parent: BIT-US-0056
milestone: BIT-M-0002
author: mcp
labels: [bitacora-config, compat]
estimate: 3
created: 2026-10-06T14:28:54Z
updated: 2026-10-06T14:38:56Z
---

## Description
In `crates/bitacora-config/src/edn/` implement (or wrap a vetted crate, MIT/Apache only, `cargo deny` clean) an EDN reader producing `EdnValue` (nil, bool, int, float, string, char, keyword, symbol, list, vector, map (ordered), set, tagged). Requirements:
- comments `;`, discard `#_`, commas as whitespace, `#{}` sets, `#"regex"` kept as raw, `(fn [r] …)` read as a list.
- Duplicate map keys → error with byte span, mirroring `handler/common/config_edn.cljs:66-73`.
- Errors carry line/column for UI diagnostics.

Prefer sharing the tokenizer with the comment-preserving CST editor (sibling story) — design `edn/lexer.rs` so both use it.

## Acceptance Criteria
- Parses our own large, heavily commented fixture config `crates/bitacora-config/tests/data/commented-config.edn` (self-authored; covers every key of [[01-file-graph-layout]] §2.2, `(fn …)` lists, sets, regexes, tagged literals, `#_` discards — no text copied from Logseq's template) and Bitacora's default config. Optionally, a dev-only ignored test parses the template from the local `../logseq` checkout as a black-box check (never vendored).
- Unit tests for each literal kind, nested maps, `#_` discard, duplicate key error.
- Fuzz-ish proptest: arbitrary bytes never panic.

## Notes
Refs BIT-SP-0002.R3. ADR-015 (Logseq's config template is not vendored).
