---
id: BIT-T-0456
type: task
title: Agent state header from STATE_SNAPSHOT/DELTA
status: done
priority: low
parent: BIT-US-0148
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, bitacora-app]
estimate: 2
created: 2026-10-07T09:19:06Z
updated: 2026-10-07T12:25:52Z
started: 2026-10-07T12:17:21Z
closed: 2026-10-07T12:25:52Z
---

## Description
Apply JSON Patch deltas to the `StateDoc`; header shows model, token usage/budget, running sub-agents.

## Acceptance Criteria
- Unit test applying recorded deltas; header updates live.
