---
id: BIT-T-0420
type: task
title: Verify AG-UI agents load external MCP servers and tool globs
status: backlog
priority: high
parent: BIT-US-0134
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, agui, mcp]
estimate: 2
created: 2026-10-07T09:15:16Z
updated: 2026-10-07T09:15:16Z
---

## Description
Go integration test: AG-UI profile with an external `streamable-http` MCP server requiring a bearer header; assert tools are listed/filtered by `Tools` globs and callable. Fix the agent service wiring if `MCPServers` are not loaded.

## Acceptance Criteria
- Test green; behaviour documented in `docs/agui.md`.
