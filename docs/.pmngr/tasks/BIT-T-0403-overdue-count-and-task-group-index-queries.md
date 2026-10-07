---
id: BIT-T-0403
type: task
title: Overdue count and task-group index queries
status: backlog
priority: medium
parent: BIT-US-0126
milestone: BIT-M-0006
author: mcp
labels: [v2, bitacora-index]
estimate: 2
created: 2026-10-07T09:13:45Z
updated: 2026-10-07T09:13:45Z
---

## Description
Add read APIs in `bitacora-index` for overdue task count and tasks within date windows (reusing the query DSL engine), with paging for large graphs.

## Acceptance Criteria
- Tests incl. rebuild-from-scratch equality; p95 < 50 ms on the large preset.
