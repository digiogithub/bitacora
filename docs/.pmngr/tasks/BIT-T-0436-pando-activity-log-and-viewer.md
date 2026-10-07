---
id: BIT-T-0436
type: task
title: Pando activity log and viewer
status: backlog
priority: medium
parent: BIT-US-0140
milestone: BIT-M-0007
author: mcp
labels: [v2, privacy, bitacora-app]
estimate: 2
created: 2026-10-07T09:16:34Z
updated: 2026-10-07T09:16:34Z
---

## Description
Bounded machine-local log (SQLite app DB or JSONL) of sync batches, runs, `pando`-token MCP calls and approvals; viewer in settings/Agent tab with clear action.

## Acceptance Criteria
- Log rotation test; entries contain ids/counts, not block text.
