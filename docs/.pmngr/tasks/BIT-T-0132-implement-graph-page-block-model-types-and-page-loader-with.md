---
id: BIT-T-0132
type: task
title: Implement Graph/Page/Block model types and page loader with origin spans
status: in_progress
priority: critical
parent: BIT-US-0029
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, core]
estimate: 3
created: 2026-10-06T14:29:59Z
updated: 2026-10-06T18:28:38Z
started: 2026-10-06T18:28:38Z
---

## Description
In `crates/bitacora-core/src/model/` add `BlockId` (u64 newtype, `slotmap` key), `Graph`, `Page`, `Block`, `Origin { span, depth, text_hash, layout: LineLayout }`, `DiskSnapshot { bytes: Arc<[u8]>, hash: blake3::Hash, mtime, len }`, `FileStyle { indent_unit, line_ending, bullet, final_newline }` as in [[block-editor]] §2.1. `Page::load(bytes, path)` builds the tree from `bitacora-markdown` outline output, filling `Origin` per block and detecting `FileStyle` (majority indent unit, CRLF/LF, final newline). `Block::is_clean()` = text hash and depth match origin.

## Acceptance Criteria
- Loading every fixture in `fixtures/graphs/**` yields a tree whose DFS depths/texts equal the parser output.
- `is_clean()` true for all blocks right after load; false after `text` change; true after same-depth reparent.
- `FileStyle` detection unit tests: tabs, 2/4 spaces, mixed (majority), CRLF, missing final newline.

## Notes
Story BIT-US-0029. Implements BIT-SP-0004.R4. No `unwrap()` on user data paths.
