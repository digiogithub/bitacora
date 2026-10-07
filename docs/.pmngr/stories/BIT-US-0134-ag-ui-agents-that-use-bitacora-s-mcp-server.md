---
id: BIT-US-0134
type: story
title: AG-UI agents that use Bitacora's MCP server
status: backlog
priority: high
parent: BIT-EP-0019
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, agui, mcp]
estimate: 5
created: 2026-10-07T09:14:29Z
updated: 2026-10-07T09:14:29Z
---

## Description
As Bitacora, I want Pando AG-UI agents to call Bitacora's MCP tools with a bearer header and restricted tool globs, and ready-made profiles for Bitacora features.

## Acceptance Criteria
- Verified (test) that the AG-UI adapter's agent service loads external `MCPServers` over `streamable-http` with `Headers`, and that `Tools` globs filter MCP tools; fixed if not.
- Example profiles `bitacora-chat`, `bitacora-journal-reviewer`, `bitacora-recommender`, `bitacora-writer` documented in `docs/agui.md` (persona, tool allow-lists, HITL on).
- Optional: per-run MCP server config passed by the client (`forwardedProps`) so Bitacora's dynamic MCP port/token need no Pando config edit.

## Notes
Pando refs: `internal/agui/doc.go` (I1), `config.go:42-70` (MCPServer), `config.go:1376-1520` (AGUIConfig, Profiles).
