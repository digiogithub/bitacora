---
id: BIT-US-0022
type: story
title: Agent audit log and "Agent activity" view with per-entry undo
status: done
priority: high
parent: BIT-EP-0010
milestone: BIT-M-0003
author: mcp
labels: [mcp, audit, ui]
estimate: 5
created: 2026-10-06T14:27:12Z
updated: 2026-10-06T21:54:02Z
started: 2026-10-06T19:40:57Z
closed: 2026-10-06T21:54:02Z
---

## Description
As a user, I want to see every tool call agents made and undo any write with one click, so that I can trust agents with write access.

## Acceptance Criteria
- Append-only JSONL audit log in the app data dir, one record per tool call and per auth failure (timestamp, token name, clientInfo, tool, args hash + summary, affected uuids, result); no note content or token values stored.
- Log rotation (e.g. 10 MB × 5 files) without losing the newest entries.
- Settings/Sidebar "Agent activity" lists entries with filter by token and tool.
- Undo on a write entry reverts it via an undoable core op; disabled (with reason) if the blocks changed since.

## Notes
Implements: BIT-SP-0007.R8, BIT-SP-0007.R7. See [[mcp-server]] §3 (Audit), §4.
