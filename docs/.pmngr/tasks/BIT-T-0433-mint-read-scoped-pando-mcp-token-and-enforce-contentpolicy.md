---
id: BIT-T-0433
type: task
title: Mint Read-scoped pando MCP token and enforce ContentPolicy
status: done
priority: high
parent: BIT-US-0139
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-mcp, security]
estimate: 2
created: 2026-10-07T09:16:34Z
updated: 2026-10-07T11:03:28Z
started: 2026-10-07T10:35:57Z
closed: 2026-10-07T11:03:28Z
---

## Description
Auto-create token `pando` (Read) in the MCP token store/keychain when Pando is enabled; optional Write scope per graph; MCP read tools filter excluded content for this token; audit entries tagged as Pando.

## Acceptance Criteria
- HTTP tests: write tool rejected with Read token; excluded page invisible.
