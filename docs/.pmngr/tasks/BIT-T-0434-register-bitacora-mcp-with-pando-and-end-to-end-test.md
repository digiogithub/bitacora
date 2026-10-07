---
id: BIT-T-0434
type: task
title: Register Bitacora MCP with Pando and end-to-end test
status: backlog
priority: high
parent: BIT-US-0139
milestone: BIT-M-0007
author: mcp
labels: [v2, pando, mcp, tests]
estimate: 3
created: 2026-10-07T09:16:34Z
updated: 2026-10-07T09:16:34Z
---

## Description
Pass MCP URL+token per run when Pando supports it; otherwise generate a config snippet / guided setup for Pando's `MCPServers`. E2E test with a real Pando (stub model) calling Bitacora read tools.

## Acceptance Criteria
- E2E test green in an opt-in CI job; setup documented.
