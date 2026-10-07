---
id: BIT-US-0148
type: story
title: Tool-call cards and agent state header
status: backlog
priority: medium
parent: BIT-EP-0022
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, agui, bitacora-app]
estimate: 5
created: 2026-10-07T09:18:16Z
updated: 2026-10-07T09:18:16Z
---

## Description
As a user, I want to see what the agent is doing: which tools it calls on my graph and with what result, and which model/budget it uses.

## Acceptance Criteria
- `TOOL_CALL_START/ARGS/END/RESULT` rendered as collapsible mono cards (name, args, result, duration, status).
- Header from `STATE_SNAPSHOT/DELTA`: model, token budget/usage, running sub-agents; `REASONING_*` collapsed by default; `ACTIVITY_*` as progress line.

## Notes
Implements BIT-SP-0011.R3.
