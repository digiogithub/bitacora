---
id: BIT-T-0096
type: task
title: Lossless EDN CST (rowan-style green tree) with byte-exact printing
status: done
priority: high
parent: BIT-US-0071
milestone: BIT-M-0002
author: mcp
labels: [bitacora-config, compat]
estimate: 3
created: 2026-10-06T14:29:19Z
updated: 2026-10-06T16:52:52Z
started: 2026-10-06T16:46:26Z
closed: 2026-10-06T16:52:52Z
---

## Description
`crates/bitacora-config/src/edn/cst.rs`: build a lossless CST from the shared lexer: nodes for map/vector/list/set/tagged/discard, leaf tokens for atoms, whitespace, commas, `;` comments. Every byte belongs to exactly one token. Provide `Cst::parse(&str)`, `Cst::print() -> String`, and navigation `top_map()`, `get(key_path)` returning node spans.

Optionally use `rowan` (MIT/Apache) for green/red trees; otherwise a simple arena of nodes with child ranges.

## Acceptance Criteria
- `print(parse(s)) == s` for our self-authored commented fixture config (`tests/data/commented-config.edn`), all fixture configs, and a proptest generator of EDN with random whitespace/comments.
- `get([":journal/page-title-format"])` on a config where that key is commented out returns `None`.

## Notes
Refs BIT-SP-0002.R4. ADR-015 (no vendored Logseq template).
