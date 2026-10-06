---
id: BIT-T-0113
type: task
title: "HTTP integration test harness: real axum server on ephemeral port + MCP client"
status: in_progress
priority: high
parent: BIT-US-0015
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, mcp, testing]
estimate: 2
created: 2026-10-06T14:29:54Z
updated: 2026-10-06T16:57:38Z
started: 2026-10-06T16:57:38Z
---

## Description
`crates/bitacora-mcp/tests/support/mod.rs`: spawn `McpServer` on port 0 with a fixture graph facade and a known token; helper `McpTestClient` using `reqwest` to POST JSON-RPC (`initialize`, `notifications/initialized`, `tools/list`, `tools/call`) with `Accept: application/json, text/event-stream`, parsing both JSON and SSE responses and carrying `Mcp-Session-Id`. Optionally also exercise rmcp's client transport for a round-trip.

## Acceptance Criteria
- `tests/lifecycle.rs`: initialize → tools/list round-trip in stateful and stateless modes.
- Harness reused by auth, tool and resource tests.

## Notes
Story BIT-US-0015. AGENTS.md §6 (bitacora-mcp HTTP tests).
