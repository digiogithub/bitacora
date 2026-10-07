---
id: BIT-T-0410
type: task
title: SSE run stream, /info, healthz and cancel
status: backlog
priority: high
parent: BIT-US-0130
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, sdk, agui]
estimate: 2
created: 2026-10-07T09:15:16Z
updated: 2026-10-07T09:15:16Z
---

## Description
`AguiClient::run(agent, input) -> Stream<Item = Result<Event>>` with an incremental SSE parser (multi-line data, comments, reconnect not automatic), `/info` and `/healthz`, `cancel(run_id)`, bearer auth, no Origin header.

## Acceptance Criteria
- Parser tests for chunk boundaries; mock-server run test; cancel test.
