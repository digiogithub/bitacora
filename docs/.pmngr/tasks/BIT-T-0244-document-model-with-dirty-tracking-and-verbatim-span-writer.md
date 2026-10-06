---
id: BIT-T-0244
type: task
title: Document model with dirty tracking and verbatim span writer
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
Implement `crates/bitacora-markdown/src/doc.rs` and `serialize.rs`: `Document { source: Bytes, pre_block: Option<Node>, blocks: Vec<Node> }` where each `Node` is either `Original { span }` (bytes copied verbatim from `source`) or `Edited { depth, content: String, drawer_origin }`. `serialize(&Document, &WriteOptions) -> Vec<u8>` walks nodes depth-first: originals are emitted exactly; edited nodes go through the canonical writer (next task). Handle boundaries: when an edited block follows an original block that had no trailing newline (EOF block, `core.cljs:108-110`), insert exactly one EOL using the line ending of the surrounding original (LF for new files). Structural ops (move/indent) mark the moved nodes as edited.

## Acceptance Criteria
- `serialize(parse(x)) == x` for all fixtures (including no trailing newline at EOF, CRLF, BOM).
- Editing the middle block of a 3-block CRLF file leaves bytes of blocks 1 and 3 identical.
- Appending a new block at the end of a file without trailing newline produces `"…last\n- new"`.

## Notes
Part of BIT-US-0092. Implements BIT-SP-0001.R12. See [[block-editor]] (byte-preserving serializer).
