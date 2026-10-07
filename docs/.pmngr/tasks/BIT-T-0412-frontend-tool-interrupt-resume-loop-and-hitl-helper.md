---
id: BIT-T-0412
type: task
title: Frontend-tool interrupt/resume loop and HITL helper
status: done
priority: high
parent: BIT-US-0130
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, sdk, agui]
estimate: 3
created: 2026-10-07T09:15:16Z
updated: 2026-10-07T10:21:43Z
closed: 2026-10-07T10:21:43Z
---

## Description
Helper that runs an agent with declared frontend tools: on `RUN_FINISHED{outcome:"interrupt"}` hands pending tool calls to a caller-provided async handler and resumes the thread with `tool` messages; helper to answer `pando_permission_request` (allow/deny) with fail-closed timeout.

## Acceptance Criteria
- Tests: interrupt → handler → resume; permission request denied on timeout.
