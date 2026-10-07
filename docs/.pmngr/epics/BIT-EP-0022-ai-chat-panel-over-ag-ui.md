---
id: BIT-EP-0022
type: epic
title: AI chat panel over AG-UI
status: backlog
priority: high
milestone: BIT-M-0008
author: mcp
labels: [v2, pando, ai, agui, bitacora-app]
created: 2026-10-07T09:10:54Z
updated: 2026-10-07T09:10:54Z
---

## Description
The Agent tab of the right panel becomes a chat with Pando over AG-UI: streaming text, `[[Page]]` links, tool-call cards, state header (model, budget), threads (list/resume/delete), cancel, context attachment (page, selection), HITL approval cards for `pando_permission_request`, and the `propose_edit` frontend tool with diff preview applied via the core Op queue.

## Acceptance Criteria
- BIT-SP-0011.R1, R2, R3 satisfied and verified against a real `pando agui-serve`.
- The existing MCP agent activity view keeps working and shows Pando-initiated writes.

## Notes
- Plan [[bitacora-v2-plan]] decision D5 (ADR-031). Pando refs: `internal/agui/server.go:46-55`, `events.go:10-41`, `frontend_tool.go`, `hitl.go:40`, `threads.go`.
