---
id: BIT-T-0403
type: task
title: Overdue count and task-group index queries
status: done
priority: medium
parent: BIT-US-0126
milestone: BIT-M-0006
author: mcp
labels: [v2, bitacora-index]
estimate: 2
created: 2026-10-07T09:13:45Z
updated: 2026-11-11T20:31:31Z
started: 2026-10-07T11:21:34Z
closed: 2026-11-11T20:31:31Z
---

## Description
Add read APIs in `bitacora-index` for overdue task count and tasks within date windows (reusing the query DSL engine), with paging for large graphs.

## Acceptance Criteria
- Tests incl. rebuild-from-scratch equality; p95 < 50 ms on the large preset.
