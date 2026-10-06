---
id: BIT-US-0020
type: story
title: Write tools through the core op pipeline with optimistic concurrency
status: backlog
priority: high
parent: BIT-EP-0010
milestone: BIT-M-0003
author: mcp
labels: [mcp, tools, write]
estimate: 13
created: 2026-10-06T14:27:12Z
updated: 2026-10-06T14:27:12Z
---

## Description
As an AI agent with write permission, I want to create pages and append, insert, update, move and annotate blocks, so that I can capture notes and maintain tasks — with every change undoable by the user exactly like their own edits.

Tools: `create_page`, `append_block`, `prepend_block`, `insert_block`, `update_block`, `set_block_property`, `remove_block_property`, `move_block`, `set_task_status`, `git_sync_now`.

## Acceptance Criteria
- Each call becomes one `Op` transaction on the core command queue with `origin = Agent{token}`; one undo entry labelled with the agent; index updated; editors notified.
- Content validated by `bitacora-markdown`: multi-block content or reserved `id::`/`collapsed::` → `INVALID_CONTENT`.
- `expected_version` mismatch → `CONFLICT` with current block; target being edited → `BLOCK_BUSY` (+`retry_after_ms`); target in sync conflict → `BLOCK_IN_CONFLICT`.
- Write targets get a persistent `id::` lazily.
- Changes are committed by sync as `Bitacora-Kind: agent` with `Bitacora-Agent` trailer.

## Notes
Implements: BIT-SP-0007.R7, BIT-SP-0007.R9, BIT-SP-0007.R10, BIT-SP-0007.R12, BIT-SP-0007.R13. See [[mcp-server]] §4, §5.2, §5.4, [[04-editor-outliner-operations]], [[block-editor]]. ADR-006, ADR-011.
