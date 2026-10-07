---
id: BIT-T-0452
type: task
title: Chat entity and AG-UI run loop bridge
status: in_review
priority: high
parent: BIT-US-0147
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, bitacora-pando]
estimate: 3
created: 2026-10-07T09:19:06Z
updated: 2026-10-07T11:51:01Z
started: 2026-10-07T11:51:01Z
---

## Description
`bitacora-pando::agent` service runs threads with frontend tools via `pando-rs`; app-side `ChatThread` entity receives events over channels, keeps message model (text, tool calls, approvals), handles cancel.

## Acceptance Criteria
- Tests with recorded SSE fixtures: message model built correctly; cancel sends `POST /runs/{id}/cancel`.
