---
id: BIT-T-0145
type: task
title: "Inline tokenizer: code/math spans, page refs with nesting, block refs"
status: done
parent: BIT-US-0083
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, parser, refs]
estimate: 3
created: 2026-10-06T14:30:15Z
updated: 2026-10-06T17:23:35Z
closed: 2026-10-06T17:23:35Z
---

## Description
Implement `crates/bitacora-markdown/src/inline/mod.rs` and `inline/refs.rs`: a single-pass byte scanner over a block's de-indented content producing `InlineToken { span, kind }` with kinds `Code`, `Math`, `PageRef { name, nested: Vec<String> }`, `BlockRef { uuid }`, `LabelledPageRef { label, name }`, `LabelledBlockRef { label, uuid }`, `Escaped`.
- Inline code (`` `x` ``, ``` ``co`de`` ```) and `$…$`, `$$…$$`, `\(..\)`, `\[..\]` are scanned first and suppress refs (code beats emphasis and links, `mldoc inline.ml:1408-1450`).
- `[[…]]` with balanced nesting: `[[a [[b]] c]]` → name `a [[b]] c` plus nested `b`.
- `((uuid))` only with a canonical UUID `[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}` (`block_ref.cljs:9`).
- `[label]([[page]])`, `[label](((uuid)))`.
- `\[[x]]` → no ref; backslash kept (`Escaped`).
- `[[assets/…]]` and `[[draws/…]]` → `FileLink` not `PageRef` (`mldoc.cljc:161-175`).

## Acceptance Criteria
- Our own unit-test vectors covering the documented cases (code vs emphasis overlaps; property references), verified black-box against mldoc 1.5.7 (no mldoc/Logseq test files copied or translated).
- Fixture 13 of §11 (all ref forms) produces the expected token list with exact byte spans.
- Fuzz test (cargo-fuzz or proptest) never panics and spans are always in-bounds char boundaries.

## Notes
Part of BIT-US-0083. Implements BIT-SP-0001.R8. ADR-015.
