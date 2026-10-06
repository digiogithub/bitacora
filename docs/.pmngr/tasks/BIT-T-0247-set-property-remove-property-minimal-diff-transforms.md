---
id: BIT-T-0247
type: task
title: set_property / remove_property minimal-diff transforms
status: in_progress
parent: BIT-US-0093
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, serializer, properties]
estimate: 3
created: 2026-10-06T14:32:01Z
updated: 2026-10-06T17:02:01Z
started: 2026-10-06T17:02:01Z
---

## Description
Implement `crates/bitacora-markdown/src/edit/properties.rs` operating on a block's de-indented content string (fence/quote aware, unlike Logseq's line-based version):
- `set_property(content, key, value) -> String`: key lowercased, value trimmed, line `key:: value` (`property.cljs:251-252`). Existing key in the effective group → replace that line in place. Otherwise append to the end of the first property group. No group → insert after the first line if it is a "title" (Paragraph/Heading/Raw_Html/Hiccup, `src/main/frontend/format/mldoc.cljs:36-41`) else at the top. If SCHEDULED/DEADLINE lines directly follow the title, insert after them (`drawer.cljs:53-68`).
- `set_property_values(content, key, &[page])` writes `[[a]], [[b]]` (`:318-334`).
- `remove_property(content, key)` removes the first matching line of the effective group (`:336-351`), never inside fences.
- Front-matter variant for pre-blocks: `key: value` (`util/property.cljs:222-224,282-285`).
Never reorder or hoist other lines (unlike `with-built-in-properties`).

## Acceptance Criteria
- Our own insert/update/remove property vectors covering the documented cases of [[02-markdown-block-syntax]], verified black-box against Logseq (document where our output deliberately differs because Logseq hoists); no Logseq test file copied or translated.
- `"- task\n  b:: 1\n  a:: 2"` set b=3 → only line 2 changes; set Status=open on `"- task\n  b:: 1\n  body text"` → `status:: open` inserted after `b:: 1`.
- Property-like lines inside a fence are never modified.

## Notes
Part of BIT-US-0093. Implements BIT-SP-0001.R15, R6. Open question §12.4 (multiple groups): only the first group is edited. ADR-015.
