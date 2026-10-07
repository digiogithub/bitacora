---
id: BIT-T-0422
type: task
title: Per-run MCP server configuration from the client
status: backlog
priority: low
parent: BIT-US-0134
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, agui, mcp]
estimate: 3
created: 2026-10-07T09:15:16Z
updated: 2026-10-07T09:15:16Z
---

## Description
Allow a client to pass an MCP server (URL restricted to loopback, bearer header) in `forwardedProps` for a run/thread, when enabled in Pando config, so Bitacora need not edit Pando's config file.

## Acceptance Criteria
- Disabled by default; Go tests for loopback restriction and header handling.
