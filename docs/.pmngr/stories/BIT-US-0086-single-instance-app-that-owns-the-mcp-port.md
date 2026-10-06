---
id: BIT-US-0086
type: story
title: Single-instance app that owns the MCP port
status: backlog
priority: high
parent: BIT-EP-0014
milestone: BIT-M-0005
author: mcp
labels: [release, bitacora-app, bitacora-cli, mcp]
estimate: 5
created: 2026-10-06T14:29:55Z
updated: 2026-10-06T14:29:55Z
---

## Description
As a user who launches Bitacora from the dock, a file manager or the command line, I want a second launch to focus the running window (and open the requested graph there) instead of starting another process, and agents' MCP clients to always reach the one process that owns the graph, so that there is never a port clash, a double writer or two indexes for the same graph.

## Acceptance Criteria
- Launching `bitacora` while it runs forwards its args (`--graph <path>`) to the running instance over a local IPC channel and exits 0; the running instance focuses/opens the graph.
- Only the instance holding the per-user lock binds the MCP port (default `127.0.0.1:12316`); if the port is busy by a foreign process, startup continues without MCP and the status bar/settings show the error (no silent port change).
- `bitacora-cli serve --graph X` refuses to start (clear message, non-zero exit) when the desktop app is running for the same user, and vice versa the app reports that a headless server holds the port.
- Closing the last window keeps the process (and MCP server) alive when "keep running in background" is on; Quit stops both.
- Integration tests cover lock acquisition, arg forwarding and stale-lock recovery after a crash.

## Notes
- [[mcp-server]] §2 (process model: server lives in the main process, always running incl. tray; port 12316; fail visibly if busy; `GET /health`).
- AGENTS.md §3 rules 3 (single writer) and 7 (bind 127.0.0.1). ADR-010, ADR-012. Related spec: BIT-SP-0007 (MCP transport/lifecycle).
