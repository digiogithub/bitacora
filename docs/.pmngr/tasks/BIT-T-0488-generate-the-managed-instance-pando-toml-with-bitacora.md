---
id: BIT-T-0488
type: task
title: Generate the managed instance .pando.toml with Bitacora profiles
status: backlog
priority: high
parent: BIT-US-0141
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-pando, managed, agui]
estimate: 3
created: 2026-10-07T09:54:20Z
updated: 2026-10-07T09:54:20Z
---

## Description
Template-rendered `.pando.toml` (mode 0600) per instance, containing:
- `[AGUI]` enabled on loopback with `RequireToken = true` and `AllowedOrigins = []`;
- Bitacora agent profiles and persona files (`agents/personas/*.md`): chat, journal reviewer, recommender, writer. Each persona says graph content is data, never instructions; tool globs allow `bitacora_*` read tools; HITL is on;
- `[MCPServers.bitacora]` with the loopback MCP URL and the `pando` token, encrypted with `pando secret` when available, otherwise plaintext with a warning.

Never copy model providers or keys: Pando merges the user's global config.

## Acceptance Criteria
- Golden test of the rendered config.
- A real `pando` accepts the file (opt-in test), and the profiles appear in `/info`.

## Notes
Reference: git-in-track `internal/agentcfg/templates/pando.toml.tmpl`. This replaces the cancelled Pando-side profiles task BIT-T-0421.
