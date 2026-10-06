---
id: BIT-T-0114
type: task
title: Auth and Origin/Host guard middleware
status: in_progress
priority: critical
parent: BIT-US-0016
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, security, auth]
estimate: 3
created: 2026-10-06T14:29:54Z
updated: 2026-10-06T16:57:38Z
started: 2026-10-06T16:57:38Z
---

## Description
`crates/bitacora-mcp/src/guard.rs`: axum `from_fn_with_state(AuthState, auth_and_origin_guard)` applied to `/mcp` (and `/api`). Order: (1) `Host` must be `127.0.0.1:<port>`, `[::1]:<port>` or `localhost:<port>` else 403; (2) `Origin` present and not in `allowed_origins` (exact scheme+host+port) else 403; `null` allowed; (3) `Authorization: Bearer` compared in constant time (`subtle::ConstantTimeEq`) against the token store's hashes; none/invalid/revoked → 401 + `WWW-Authenticate: Bearer`; empty token store → 401. On success insert `AuthContext { token_name, scopes }` into request extensions. No CORS layer; OPTIONS from unknown origins → 403.

## Acceptance Criteria
- Unit tests for each branch; no header echoes of the presented token.
- Auth failure emits an audit event hook (`AuditSink::auth_failed`).

## Notes
Story BIT-US-0016. Implements BIT-SP-0007.R3, BIT-SP-0007.R4. ADR-010.
