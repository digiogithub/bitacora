---
id: BIT-US-0139
type: story
title: Provision Bitacora MCP access for Pando agents
status: in_progress
priority: high
parent: BIT-EP-0020
milestone: BIT-M-0007
author: mcp
labels: [v2, pando, mcp, bitacora-mcp, security]
estimate: 5
created: 2026-10-07T09:15:49Z
updated: 2026-10-07T10:35:50Z
started: 2026-10-07T10:35:50Z
---

## Description
As a user, I want Pando agents to read my graph through Bitacora's MCP server with a least-privilege token, so that agent access is audited and limited.

## Acceptance Criteria
- A dedicated `pando` MCP token minted automatically (keychain), Read scope by default; optional Write grant per graph in settings; `ContentPolicy` exclusions enforced in MCP readers for this token.
- Bitacora's MCP endpoint registered with Pando (per-run config when supported, otherwise documented/assisted Pando config).
- E2E test: Pando agent (stub model) calls `bitacora` read tools successfully and a write tool is rejected.

## Notes
Implements BIT-SP-0011.R1. Bitacora MCP scopes in `crates/bitacora-mcp/src/tokens.rs`.
