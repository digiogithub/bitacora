---
id: BIT-US-0016
type: story
title: Bearer token auth, Origin/Host checks and token management
status: in_progress
priority: critical
parent: BIT-EP-0010
milestone: BIT-M-0002
author: mcp
labels: [mcp, security, auth]
estimate: 8
created: 2026-10-06T14:27:12Z
updated: 2026-10-06T16:57:38Z
started: 2026-10-06T16:57:38Z
---

## Description
As a user, I want every MCP request to require a token stored safely outside my graph and to be refused from browser pages, so that no web page or unauthorised local process can read or modify my notes.

Context: Logseq's HTTP API is open when no token is configured and sends `Access-Control-Allow-Origin: *` ([[05-git-and-apis]] §2.3). Bitacora must never do either.

## Acceptance Criteria
- Requests without/with wrong/revoked token get `401` + `WWW-Authenticate: Bearer`; empty token list refuses everything.
- Foreign `Origin` or non-loopback `Host` → `403`; no permissive CORS headers ever.
- Default 256-bit token generated on first run, stored in OS keychain with `0600` file fallback in app config dir; never inside the graph.
- Settings > Agents lists named tokens with scopes, create/revoke, and "copy config snippet" for Claude Desktop/Code.
- HTTP integration tests cover all rejection paths.

## Notes
Implements: BIT-SP-0007.R3, BIT-SP-0007.R4, BIT-SP-0007.R5, BIT-SP-0007.R6. See [[mcp-server]] §3, §8. ADR-010.
