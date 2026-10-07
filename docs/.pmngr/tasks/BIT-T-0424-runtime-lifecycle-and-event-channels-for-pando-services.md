---
id: BIT-T-0424
type: task
title: Runtime lifecycle and event channels for Pando services
status: done
priority: high
parent: BIT-US-0135
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-runtime]
estimate: 2
created: 2026-10-07T09:16:33Z
updated: 2026-10-07T10:29:46Z
closed: 2026-10-07T10:29:46Z
---

## Description
`bitacora-runtime` starts/stops the Pando service per graph session according to settings, exposes status/progress/run events to `bitacora-app` (tokio bridge + async_channel) and `bitacora-cli`; shutdown ordering after MCP and before index close.

## Acceptance Criteria
- Runtime tests: enable/disable at runtime, shutdown with pending work does not hang.
