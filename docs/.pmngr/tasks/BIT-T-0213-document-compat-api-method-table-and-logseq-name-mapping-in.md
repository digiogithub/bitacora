---
id: BIT-T-0213
type: task
title: Document compat API method table and Logseq name mapping in mcp-server design
status: backlog
priority: low
parent: BIT-US-0023
milestone: BIT-M-0003
author: mcp
labels: [bitacora-mcp, docs, compat]
estimate: 1
created: 2026-10-06T14:31:00Z
updated: 2026-10-06T14:31:00Z
---

## Description
Update `docs/design/mcp-server.md` with the supported `POST /api` method table, argument adapters, error format and known differences (no Datalog, no UI methods). Ensure every MCP tool description names its Logseq equivalent; add a doc test listing tool → Logseq method.

## Acceptance Criteria
- Doc updated; "Open questions" item 2 of [[05-git-and-apis]] answered or cross-referenced.

## Notes
Story BIT-US-0023. Implements BIT-SP-0007.R17.
