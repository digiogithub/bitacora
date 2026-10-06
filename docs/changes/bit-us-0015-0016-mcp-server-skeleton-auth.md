---
created_at: 2026-10-06T17:03:48.537295203Z
updated_at: 2026-10-06T17:03:48.537295203Z
tags:
    - change
    - mcp
---
# MCP server skeleton + auth (BIT-US-0015, BIT-US-0016)

Plan: [[bitacora-full-development-plan]]. Design: [[mcp-server]] (ADR-010).

## What changed (commit 7b13004)
- `crates/bitacora-mcp/src/server.rs`: `McpServer::start/stop`, `McpConfig` (loopback bind, port 12316, allowed_origins, stateful/stateless). Dedicated 2-thread tokio runtime; busy port -> `Error::PortInUse`, non-loopback -> `Error::NonLoopbackBind`. `GET /health` -> `{"ok":true}`.
- `handler.rs`: rmcp `BitacoraMcp` with `ping` and `get_graph_info` tools; `reader.rs`: `GraphReader` trait, `GraphInfo`, `StaticGraphReader`.
- `guard.rs`: Host (loopback:port) 403, Origin allowlist 403 (`null` rejected), bearer 401 + `WWW-Authenticate`; no CORS headers.
- `tokens.rs`: `TokenStore` (256-bit `bit_` tokens, named, scopes read/write/delete, create/revoke/rotate, constant-time verify via `subtle`, atomic 0600 file at `<config dir>/mcp-tokens.json`).
- `crates/bitacora-cli/src/main.rs`: `serve --graph <path> [--port] [--token-file] [--allow-origin]`.
- Deps added: subtle, getrandom (workspace pins), directories, tokio/axum/rmcp in mcp.

## Not done
OS keychain storage, Settings > Agents UI (BIT-T-0116), desktop app start, scope enforcement per tool, `gintrack spec ingest` for verify_requirement.

## Verification
cargo clippy --workspace --all-targets --locked -D warnings clean; cargo test -p bitacora-mcp: 7 unit + 10 HTTP integration pass; cargo deny, xtask check-deps, machete clean; manual CLI smoke: /health 200, /mcp 401; `cargo tree -p bitacora-core` has no tokio/rmcp.
