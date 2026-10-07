---
id: BIT-T-0459
type: task
title: Fail-closed handling of pending approvals
status: backlog
priority: high
parent: BIT-US-0149
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, security]
estimate: 2
created: 2026-10-07T09:19:06Z
updated: 2026-10-07T09:19:06Z
---

## Description
Timeout, panel close, thread switch, graph switch and quit resolve pending approvals as denied; Pando-side timeout also handled.

## Acceptance Criteria
- Tests for each trigger; no write happens after denial.
