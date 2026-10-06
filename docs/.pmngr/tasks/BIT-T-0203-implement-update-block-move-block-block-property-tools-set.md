---
id: BIT-T-0203
type: task
title: Implement update_block, move_block, block property tools, set_task_status and git_sync_now
status: done
priority: high
parent: BIT-US-0020
milestone: BIT-M-0003
author: mcp
labels: [bitacora-mcp, tools, write]
estimate: 3
created: 2026-10-06T14:31:00Z
updated: 2026-10-06T19:40:50Z
closed: 2026-10-06T19:40:50Z
---

## Description
`crates/bitacora-mcp/src/tools/edit_blocks.rs`: `update_block {uuid, content, properties?(merge), expected_version?}`; `move_block {uuid, target_uuid, position, expected_version?}` (reject moving into own subtree); `set_block_property` (`idempotentHint`) / `remove_block_property {uuid, key, value?}`; `set_task_status {uuid, status}` editing the marker via core marker op; `git_sync_now` sends a `SyncCommand::SyncNow` through the facade (no git args accepted).

## Acceptance Criteria
- Tests: stale `expected_version` → `CONFLICT` with current content; move cycle → `INVALID_CONTENT`; property merge keeps other keys.

## Notes
Story BIT-US-0020. Implements BIT-SP-0007.R9, BIT-SP-0007.R12. Mirrors `Editor.updateBlock`, `moveBlock`, `upsertBlockProperty`, `removeBlockProperty`.
