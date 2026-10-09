---
id: BIT-US-0186
type: story
title: "Sidebar MCP server row: click to copy connection details for any MCP client"
status: backlog
priority: medium
author: mcp
labels: [bitacora-app, mcp, ui, feedback]
created: 2026-10-09T12:31:45Z
updated: 2026-10-09T12:31:45Z
---

## Description
Owner request (2026-10-09). The "MCP server" info at the bottom of the sidebar (`crates/bitacora-app/src/views/sidebar.rs` `footer` ~494, `mcp_endpoint` ~109) is display-only. Clicking it should copy what a client needs to connect (endpoint `http://127.0.0.1:<port>/mcp`, default port 12316, `bitacora-mcp` `DEFAULT_PORT`, plus `Authorization: Bearer <token>`).

## Acceptance Criteria
- Clicking the MCP row opens a small popover (when the server is running) with: endpoint URL, and copy actions "Copy URL", "Copy JSON config (mcpServers)" and "Copy `claude mcp add` command", plus "Manage tokens…" opening Settings → Agents.
- Reuses the existing snippet builders (`views/settings/agents.rs` `snippet` ~251, `model::client_config_json` / `client_config_command`); no duplicated formatting.
- Token selection: uses the default/first enabled token; if none exists, offers to create one (or links to Settings → Agents) instead of copying a config without auth.
- Success toast "Copied"; when the server is off or the port is in use, the popover says so and links to Settings → Agents.
- Secrets never logged; copying the token is explicit (the URL-only copy contains no token).
- Tests: popover content per state (running / off / no token), copied text equals the Settings snippets.

## Notes
Security rules from AGENTS.md §5.7 (127.0.0.1 only, bearer token) unchanged.
