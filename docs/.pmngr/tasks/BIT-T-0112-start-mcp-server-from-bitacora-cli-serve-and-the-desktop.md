---
id: BIT-T-0112
type: task
title: Start MCP server from bitacora-cli serve and the desktop app (tray mode)
status: in_review
priority: high
parent: BIT-US-0015
milestone: BIT-M-0002
author: mcp
labels: [bitacora-cli, bitacora-app, mcp]
estimate: 2
created: 2026-10-06T14:29:54Z
updated: 2026-10-06T17:03:39Z
started: 2026-10-06T16:57:38Z
---

## Description
Wire `McpServer::start` into `crates/bitacora-cli/src/serve.rs` (`bitacora-cli serve --graph <path> [--port N]`, logs status, exits on Ctrl-C) and into `crates/bitacora-app` startup (tokio bridge), keeping it alive when the last window closes to tray. Show `McpStatus` in Settings > Agents ("Running on 127.0.0.1:12316" / "Port 12316 is in use").

## Acceptance Criteria
- `cargo run -p bitacora-cli -- serve --graph fixtures/graphs/basic` serves `/mcp`.
- Closing the main window to tray keeps `/health` responding.
- Port-in-use status visible in settings.

## Notes
Story BIT-US-0015. Implements BIT-SP-0007.R1.
