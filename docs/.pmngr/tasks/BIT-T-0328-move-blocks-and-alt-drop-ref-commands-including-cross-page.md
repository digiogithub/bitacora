---
id: BIT-T-0328
type: task
title: Move-blocks and Alt-drop ref commands including cross-page transactions
status: done
priority: medium
parent: BIT-US-0106
milestone: BIT-M-0005
author: mcp
labels: [bitacora-core, editor]
estimate: 3
created: 2026-10-06T14:34:02Z
updated: 2026-10-06T22:30:36Z
closed: 2026-10-06T22:30:36Z
---

## Description
`crates/bitacora-core/src/commands/move_blocks.rs`: `MoveBlocks { uuids, target, position }` for selections (keep relative order, move subtrees), same-page and cross-page (multi-file transaction, one undo step, UUIDs kept since it is a move). `InsertRefAt { source_uuid, target, position }` for Alt-drop: ensure `id::` on the source (`EnsureUuid`) and insert a new block `((uuid))`.

## Acceptance Criteria
- Op-level tests: move to before/child/after, cross-page move writes both files once, undo restores both files byte-exact; Alt-drop writes `id::` exactly once.

## Notes
ADR-006. [[04-editor-outliner-operations]] Requirements 6, 7, 17; [[block-editor]] §3, §5.3.
