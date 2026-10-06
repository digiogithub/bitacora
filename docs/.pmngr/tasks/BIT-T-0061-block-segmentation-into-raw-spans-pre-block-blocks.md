---
id: BIT-T-0061
type: task
title: Block segmentation into raw spans (pre-block + blocks)
status: done
parent: BIT-US-0026
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, parser]
estimate: 3
created: 2026-10-06T14:28:29Z
updated: 2026-10-06T16:59:58Z
started: 2026-10-06T16:46:41Z
closed: 2026-10-06T16:59:58Z
---

## Description
Implement `crates/bitacora-markdown/src/outline.rs` with `pub fn split(input: &[u8]) -> Outline` producing:
- `pre_block: Option<Span>` covering `[0, first_block_start)` (whole file when there is no block line; includes BOM and YAML front matter bytes);
- `blocks: Vec<RawBlock { span, raw_level, indent: Span, head_line: Span, body: Span, kind: Bullet | AtxHeading{size} }>`.
`raw_level = indent chars + 1` (each tab/space counts 1); ATX headings without bullet are forced to level 1 (`block.cljs:568-572`). A block's span runs from its start line to the start of the next block line (blank lines between blocks belong to the previous block, `block.cljs:665-670`). Also implement `content_of(block)` following `get-block-content` (`block.cljs:406-422`): strip `^[-]+\s?` from the first line (`text.cljs:51-77`) and `remove-indentation-spaces(content, level+1)` (`mldoc.cljc:77-100`) on continuation lines, returning a de-indented `Cow<str>` used for semantics only — never for writing untouched blocks.

## Acceptance Criteria
- Invariant test: `pre_block ++ blocks.span` concatenation == input for every fixture.
- `content_of` matches our own vectors covering the documented block-content extraction cases, verified black-box against Logseq/mldoc (no Logseq test file copied or translated).
- File with no bullets → single pre-block; file starting with `- a` → no pre-block.

## Notes
Part of BIT-US-0026. Implements BIT-SP-0001.R1, R3 (pre-block span), R12 (raw spans enable verbatim writes). ADR-015.
