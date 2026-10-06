---
id: BIT-US-0026
type: story
title: Lossless line-based outline splitter and block tree
status: backlog
priority: critical
parent: BIT-EP-0003
milestone: BIT-M-0002
author: mcp
labels: [markdown, compat, parser]
estimate: 8
created: 2026-10-06T14:28:00Z
updated: 2026-10-06T14:28:00Z
---

## Description
As a developer of Bitacora, I want a line-based outline splitter that segments a Logseq Markdown page into blocks exactly like mldoc 1.5.7 while keeping every block's raw bytes, so that the rest of the system (index, editor, merge) works on the same tree as Logseq and can write untouched blocks back verbatim.

This is "Layer 1" of the parser recommended in [[02-markdown-block-syntax]] §9: scan lines, track open fences and `#+BEGIN_X` regions, detect bullet and top-level ATX heading lines, compute raw levels, and build the parent/child tree with Logseq's tolerant relative-indent algorithm (`block.cljs:695-768`). Each block records its byte range, raw indent string, bullet line and body lines; offsets are UTF-8 byte offsets; CRLF is tolerated.

## Acceptance Criteria
- `- ` / `-` at EOL lines outside fences and `#+BEGIN` regions start blocks; `-foo`, `*`, `+`, `1.` lines do not.
- Top-level `## x` lines are level-1 heading blocks; `#foo` is not a heading.
- The tree for `"- line1\n    - line2\n      - line3\n     - line4"` matches Logseq (`extract_test.cljs:81-87`), and `"## hello\n    - world"` nests `world` under the heading.
- Concatenating the raw spans of pre-block + all blocks reproduces the input bytes exactly (CRLF, BOM, tabs/spaces mixtures included).
- Block spans and positions are byte offsets; `"- café [[Señor]]\n- b"` puts the second block at byte 19.

## Notes
Implements: BIT-SP-0001.R1, BIT-SP-0001.R2, BIT-SP-0001.R19 (and the pre-block span part of BIT-SP-0001.R3).
See [[02-markdown-block-syntax]] §2, §8, §9; [[block-editor]]; ADR-003. Open question §12.1 (unclosed fence) is resolved in a task below with fixtures.
