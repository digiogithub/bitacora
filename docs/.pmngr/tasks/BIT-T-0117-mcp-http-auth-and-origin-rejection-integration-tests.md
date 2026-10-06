---
id: BIT-T-0117
type: task
title: MCP HTTP auth and Origin rejection integration tests
status: backlog
priority: critical
parent: BIT-US-0016
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, security, testing]
estimate: 2
created: 2026-10-06T14:29:55Z
updated: 2026-10-06T14:29:55Z
---

## Description
`crates/bitacora-mcp/tests/auth.rs` using the ephemeral-port harness: no token → 401 + `WWW-Authenticate: Bearer`; wrong token → 401; revoked → 401; empty store → 401; `Origin: https://evil.example` with valid token → 403 and no `Access-Control-Allow-Origin`; `Host: attacker.example:<port>` → 403; preflight `OPTIONS` from unknown origin → 403; native client without Origin → 200; allowlisted origin → 200; `/health` without token → 200 `{"ok":true}`. Also assert the server does not listen on a non-loopback interface (bind check).

## Acceptance Criteria
- All cases pass on Linux/macOS/Windows CI.
- Handler invocation counter proves rejected requests never reach rmcp.

## Notes
Story BIT-US-0016. Verifies BIT-SP-0007.R2, BIT-SP-0007.R3, BIT-SP-0007.R4.
