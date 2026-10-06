---
id: BIT-T-0115
type: task
title: "Token store: keychain with 0600 file fallback, named tokens and scopes"
status: backlog
priority: critical
parent: BIT-US-0016
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, bitacora-config, security]
estimate: 3
created: 2026-10-06T14:29:54Z
updated: 2026-10-06T14:29:54Z
---

## Description
`crates/bitacora-mcp/src/tokens.rs`: `TokenStore` with `list()`, `create(name, scopes) -> PlainToken` (32 random bytes via `rand::rngs::OsRng`, base64url), `revoke(name)`, `verify(presented) -> Option<TokenInfo>`. Secrets in OS keychain via `keyring` (service `bitacora`, account `mcp/<name>`); metadata (name, scopes, created_at, revoked) in `<config_dir>/bitacora/mcp-tokens.json`; when keychain unavailable, secrets go to the same file with mode `0600` (Unix) / user-only ACL (Windows) and a warning. Default token "default" (scopes read, write) generated on first run. Scopes enum `Read | Write | Delete`. Path resolution must reject any location inside the graph folder.

## Acceptance Criteria
- Tests with a mock keyring backend and fallback file (permissions asserted on Unix).
- Token values never appear in logs (`Debug` impl redacts).

## Notes
Story BIT-US-0016. Implements BIT-SP-0007.R5, BIT-SP-0007.R6.
