---
id: BIT-T-0091
type: task
title: Property line and group scanner with byte spans
status: backlog
parent: BIT-US-0058
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, parser, properties]
estimate: 3
created: 2026-10-06T14:29:17Z
updated: 2026-10-06T14:37:38Z
---

## Description
Implement `crates/bitacora-markdown/src/properties/scan.rs`. For a block body (raw lines from the outline splitter) produce `PropertyGroup { span, lines: Vec<PropLine { span, key_raw, key_norm, value_raw, valid }> }`:
- Line grammar (`mldoc:lib/syntax/markdown_property.ml`): `ws* key "::" (" " ws* value | ws* EOL)`, key = 1+ bytes that are not `:` or whitespace; value = rest of line trimmed (CR excluded).
- Consecutive property lines form one group; adjacent `#+name: value` lines join the group (mldoc `drawer.ml`, test `"a:: 1\n#+b: 2"`).
- Skip lines inside quotes (`> `), fences and `#+BEGIN` regions (reuse region tracking from the line scanner).
- Recognise groups anywhere in the body; expose all groups but mark the first as `effective` (`block.cljs:677-679`).
- Key normalisation (`block.cljs:204-238`): lowercase, ` `/`_` → `-`, `custom_id`/`custom-id` → `id`. Validity (`property.cljs:22-28`): valid EDN keyword chars, no `"^(){}`, not starting with `#`.

## Acceptance Criteria
- Our own unit-test vectors covering the documented property-drawer and property-extraction cases of [[02-markdown-block-syntax]], verified black-box against mldoc/Logseq (no test files copied or translated from mldoc or Logseq).
- Edge keys fixture 8 of §11 (`a.b.c::`, `empty::`, `my key:: v`, `key::value`) behaves as BIT-SP-0001.R4.
- `"- a\n  text\n  late:: prop"` → effective group with `late`.
- Invalid keys are listed separately, spans still recorded.

## Notes
Part of BIT-US-0058. Implements BIT-SP-0001.R4, R6. ADR-015.
