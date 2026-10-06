---
id: BIT-US-0015
type: story
title: Always-on MCP Streamable HTTP server skeleton
status: in_progress
priority: high
parent: BIT-EP-0010
milestone: BIT-M-0002
author: mcp
labels: [mcp, api, server]
estimate: 5
created: 2026-10-06T14:27:12Z
updated: 2026-10-06T16:57:31Z
started: 2026-10-06T16:57:31Z
---

## Description
As an AI agent user, I want Bitacora to run an MCP server over Streamable HTTP whenever the app (or `bitacora-cli serve`) is running, so that Claude Desktop/Code and other clients can connect to `http://127.0.0.1:12316/mcp` without a window being open.

Context: `rmcp` (~3.5) `StreamableHttpService` nested in an axum 0.8 router on a dedicated tokio runtime; rmcp types confined to an internal transport module; core stays synchronous.

## Acceptance Criteria
- `bitacora-cli serve --graph <path>` and the desktop app (incl. tray mode) both start the server; `initialize` succeeds with a valid token.
- Binds only `127.0.0.1` and `::1` on `mcp.port` (default 12316); a busy port is reported in status/settings and no fallback port is used.
- `GET /health` returns `{"ok":true}` without auth.
- Stateful sessions (`LocalSessionManager`) and stateless JSON-response requests both work.
- `cargo tree -p bitacora-core` contains no tokio/rmcp.

## Notes
Implements: BIT-SP-0007.R1, BIT-SP-0007.R2, BIT-SP-0007.R20. See [[mcp-server]] §1–2, [[05-git-and-apis]] §2.2. ADR-010, ADR-012.
