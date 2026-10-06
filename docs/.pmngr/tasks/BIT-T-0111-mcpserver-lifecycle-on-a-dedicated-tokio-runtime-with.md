---
id: BIT-T-0111
type: task
title: McpServer lifecycle on a dedicated tokio runtime with loopback binding
status: done
priority: high
parent: BIT-US-0015
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, mcp, server]
estimate: 3
created: 2026-10-06T14:29:54Z
updated: 2026-10-06T17:03:39Z
started: 2026-10-06T16:57:38Z
closed: 2026-10-06T17:03:39Z
---

## Description
`crates/bitacora-mcp/src/server.rs`: `McpServer::start(cfg: McpConfig, facade: Arc<dyn CoreFacade>) -> Result<McpHandle, McpError>` builds a multi-thread tokio runtime (2 workers, thread name `bitacora-mcp`), binds `TcpListener` on `127.0.0.1:<port>` and `[::1]:<port>` (IPv6 failure is a warning, IPv4 failure is fatal), runs `axum::serve(..).with_graceful_shutdown(..)`. `McpHandle { status: watch::Receiver<McpStatus>, shutdown() }` with `McpStatus::{Starting, Running{port}, PortInUse{port}, Failed(String), Stopped}`. Port busy → `PortInUse`, never another port.

## Acceptance Criteria
- Test: start on ephemeral port, `/health` reachable; second server on same port reports `PortInUse`.
- Test: listener addresses are loopback only.
- `shutdown()` completes within 2 s with in-flight requests drained.

## Notes
Story BIT-US-0015. Implements BIT-SP-0007.R1, BIT-SP-0007.R2. ADR-010, ADR-012.
