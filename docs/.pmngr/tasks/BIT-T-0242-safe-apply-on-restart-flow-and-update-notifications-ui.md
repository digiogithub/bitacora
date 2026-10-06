---
id: BIT-T-0242
type: task
title: Safe apply-on-restart flow and update notifications UI
status: in_review
priority: medium
parent: BIT-US-0100
milestone: BIT-M-0005
author: mcp
labels: [auto-update, ui, bitacora-app]
estimate: 3
created: 2026-10-06T14:31:45Z
updated: 2026-10-06T20:18:19Z
started: 2026-10-06T20:18:19Z
---

## Description
- Notification (GPUI Kit `Notification`) "Bitacora X.Y.Z is available" with "Release notes", "Download" (notice backend, opens browser via `open`) or "Restart to update" (Velopack backend after download).
- Before applying: flush the editor buffer, drain the core command queue / writer, wait for git sync to be idle (with timeout and a visible "waiting for sync" state), stop the MCP server, release the instance lock, then call Velopack's apply-and-restart. If anything fails, abort the update and keep running.

## Acceptance Criteria
- `#[gpui::test]`/integration test with a fake backend proves the pre-apply sequence order and abort-on-failure.
- Manual test: update while a block is being edited loses no text.

## Notes
- AGENTS.md §3 rules 1 and 3; [[block-editor]] §7.2 (buffer flush triggers); [[mcp-server]] §2.
