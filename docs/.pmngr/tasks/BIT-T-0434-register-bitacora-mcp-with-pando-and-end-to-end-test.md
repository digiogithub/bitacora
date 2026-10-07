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
updated: 2026-10-07T09:55:09Z
---

## Description
- **Managed mode (default):** the Bitacora MCP server is registered through `[MCPServers.bitacora]` in the generated instance `.pando.toml` (BIT-T-0488), with the loopback URL and the `pando` token; the config is regenerated when the MCP port or token changes.
- **External mode:** show a copyable config snippet for the user's Pando.
- End-to-end test with a real Pando (stub model) calling Bitacora read tools.

## Acceptance Criteria
- E2E test green in an opt-in CI job; external-mode setup documented.
