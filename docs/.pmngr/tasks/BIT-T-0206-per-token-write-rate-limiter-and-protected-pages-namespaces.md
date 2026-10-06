---
id: BIT-T-0206
type: task
title: Per-token write rate limiter and protected pages/namespaces
status: done
priority: high
parent: BIT-US-0021
milestone: BIT-M-0003
author: mcp
labels: [bitacora-mcp, security]
estimate: 2
created: 2026-10-06T14:31:00Z
updated: 2026-10-06T19:40:50Z
closed: 2026-10-06T19:40:50Z
---

## Description
`crates/bitacora-mcp/src/limits.rs`: token-bucket per token name (60 ops/min default, configurable), returning `RATE_LIMITED{retry_after_ms}`; batch cap 200 blocks. Protection check resolves target page(s) of each op and refuses when page has `bitacora-agent-readonly:: true` or name starts with any `mcp.protected_namespaces` entry + `/` (case-insensitive) → `FORBIDDEN_SCOPE`. Applies to moves (source and destination pages).

## Acceptance Criteria
- Tests with injectable clock: 61st op in a minute limited; refill works; protected namespace and property cases; move into protected page refused.

## Notes
Story BIT-US-0021. Implements BIT-SP-0007.R14.
