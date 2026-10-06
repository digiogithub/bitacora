---
id: BIT-US-0033
type: story
title: Backspace and Delete merge blocks while preserving referenced identity
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
As a note taker, I want Backspace at the start of a block to join it with the previous block and Delete at the end to pull in the next one, with Logseq's refusal rules, so that I can restructure text without breaking `((block refs))` pointing to the merged block.

## Acceptance Criteria
- `MergeWithPrevious` (Backspace @0): `SetText(prev, prev+text)`, `AdoptChildren(id→prev)`, `RemoveSubtree(id)`; caret at the junction.
- Refused when both blocks have children; refused for the first page block unless empty (empty → deleted).
- `MergeNext` (Delete @end) pulls first child or next sibling; refused if that block has children.
- When the removed block has a referenced `id::`, the survivor adopts it; refused if both have referenced uuids.
- Refusals show a non-blocking notice and produce no transaction.
- Undo restores both blocks byte-exactly, including `id::` lines.

## Notes
Implements: BIT-SP-0004.R7, BIT-SP-0004.R8.
See [[block-editor]] §3.2 and Open questions (uuid adoption), [[04-editor-outliner-operations]] §3 (`delete-block!` `editor.cljs:825-869`, `delete-concat` `:2659-2702`). ADR-006.
