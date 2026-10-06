---
id: BIT-T-0348
type: task
title: Conflicted page state and resolution commands in core
status: backlog
priority: high
parent: BIT-US-0070
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, io]
estimate: 3
created: 2026-10-06T14:34:14Z
updated: 2026-10-06T14:34:14Z
---

## Description
Add `PageState::Conflicted { disk: DiskSnapshot }` (writer skips the page; other pages unaffected). Commands: `ResolveKeepMine { page }` → backup disk bytes, set snapshot to current disk, write ours (fresh precheck); `ResolveTakeDisk { page }` → backup our serialized bytes, reload disk version, push an undoable transaction whose undo restores ours; `ConflictDiff { page } -> BlockDiff` (aligned blocks with status). MCP write commands targeting a conflicted page return `CommandError::PageConflicted`.

## Acceptance Criteria
- Tests for each resolution including backup files and undo after Take disk.
- MCP-origin command refused with the explicit error.

## Notes
Story BIT-US-0070. Implements BIT-SP-0005.R16, BIT-SP-0005.R9. [[mcp-server]].
