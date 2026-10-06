---
id: BIT-T-0110
type: task
title: Scaffold bitacora-mcp transport module with rmcp StreamableHttpService on axum
status: in_progress
priority: high
parent: BIT-US-0015
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, mcp, server]
estimate: 3
created: 2026-10-06T14:29:54Z
updated: 2026-10-06T16:57:38Z
started: 2026-10-06T16:57:38Z
---

## Description
Create `crates/bitacora-mcp/src/transport/mod.rs` (only module importing `rmcp`): `BitacoraMcp` handler struct (`#[tool_router]` / `#[tool_handler] impl ServerHandler`, `get_info` with name `bitacora`, version, instructions, capabilities tools+resources+prompts), and `fn build_router(state: McpState) -> axum::Router` nesting `StreamableHttpService::new(factory, LocalSessionManager::default().into(), StreamableHttpServerConfig::default())` at `/mcp` plus `GET /health` → `{"ok":true}`. Dependencies in workspace: `rmcp = "~3.5"` with features `server, macros, schemars, transport-streamable-http-server, transport-streamable-http-server-session`, `axum = "0.8"`, `tokio`. Define a `CoreFacade` trait (`crates/bitacora-mcp/src/facade.rs`) that the handlers use to reach core/index so tests can inject a fixture graph.

## Acceptance Criteria
- `cargo build -p bitacora-mcp` passes; `cargo deny check` passes.
- No other module imports `rmcp` (grep check in CI script or test).
- Unit test: router responds to `/health`.

## Notes
Story BIT-US-0015. Implements BIT-SP-0007.R20. Verify rmcp type names on docs.rs. ADR-010.
