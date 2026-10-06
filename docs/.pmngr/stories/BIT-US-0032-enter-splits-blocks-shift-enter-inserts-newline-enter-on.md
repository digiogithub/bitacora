---
id: BIT-US-0032
type: story
title: Enter splits blocks, Shift+Enter inserts newline, Enter on empty last child outdents
status: backlog
priority: critical
parent: BIT-EP-0007
milestone: BIT-M-0003
author: mcp
labels: [editor, outliner, bitacora-core, bitacora-app]
estimate: 5
created: 2026-10-06T14:28:07Z
updated: 2026-10-06T14:28:07Z
---

## Description
As a note taker, I want Enter to create a new block exactly like Logseq (split at caret, first-child rule, insert-before at offset 0, outdent on empty last child) and Shift+Enter to add a line inside the block, so that I can outline quickly.

## Acceptance Criteria
- `SplitBlock { id, cursor }` plans `SetText(head)` + `InsertSubtree(tail)`; tail is left-trimmed; new block is first child when the block has expanded children, otherwise next sibling.
- Caret at 0 with text after inserts an empty block before and keeps the caret in the original block.
- `Enter` on an empty last child produces a `Move` (outdent).
- `Shift+Enter` inserts `\n` in the buffer; the file shows a continuation line with the file's indent + 2 spaces.
- Enter in a code fence inserts a newline; Enter inside `[[page]]` with popup closed does not split inside the brackets (jumps past `]]`).
- Each split is one undo step; undo restores exact bytes.

## Notes
Implements: BIT-SP-0004.R6.
See [[block-editor]] §3.2, [[04-editor-outliner-operations]] §3 (`keydown-new-block` `editor.cljs:2449-2514`). ADR-002, ADR-006.
