---
id: BIT-T-0062
type: task
title: Re-implement the tolerant relative-indent tree builder
status: done
parent: BIT-US-0026
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, parser]
estimate: 2
created: 2026-10-06T14:28:29Z
updated: 2026-10-06T16:59:58Z
started: 2026-10-06T16:46:41Z
closed: 2026-10-06T16:59:58Z
---

## Description
Implement `crates/bitacora-markdown/src/tree.rs`: `fn build_tree(blocks: &[RawBlock]) -> Vec<NodeLinks { parent: Option<usize>, prev_sibling: Option<usize> }>`, re-implementing the tolerant relative-indent tree building from the documented behaviour in [[02-markdown-block-syntax]] (Logseq reference for behaviour only: `with-parent-and-left`, `deps/graph-parser/src/logseq/graph_parser/block.cljs:695-768`):
- a block with level greater than its predecessor becomes the predecessor's child, whatever the delta (`:723-738`);
- on outdent, walk the ancestor stack to the nearest ancestor with level ≤ current; equal → sibling; if none matches exactly, become a sibling of the first deeper ancestor (`:740-766`);
- ATX headings are level 1, bullets under them become children.
Expose `depth` (1-based tree depth) for each node, which the serializer uses for canonical prefixes.

## Acceptance Criteria
- Our own test vectors covering the documented cases (headings vs bullets; irregular indent such as `"- line1\n    - line2\n      - line3\n     - line4"`, Logseq regression #1902) with expected parent/child relations verified black-box against Logseq/mldoc.
- Mixed tabs/spaces file builds the same tree as Logseq (fixture `fixtures/markdown/mixed-indent.md`).
- Property test: tree is always a forest (no cycles), parents precede children.

## Notes
Part of BIT-US-0026. Implements BIT-SP-0001.R1, R2. ADR-015 (no Logseq code or test files copied/translated).
