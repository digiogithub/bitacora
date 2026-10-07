---
id: BIT-US-0146
type: story
title: Semantic search over MCP and CLI
status: in_review
priority: medium
parent: BIT-EP-0021
milestone: BIT-M-0007
author: mcp
labels: [v2, search, bitacora-mcp, bitacora-cli]
estimate: 3
created: 2026-10-07T09:16:57Z
updated: 2026-10-07T11:21:04Z
started: 2026-10-07T11:21:00Z
---

## Description
As an external agent or headless user, I want semantic search through MCP and the CLI.

## Acceptance Criteria
- MCP read-only tools `semantic_search {query, limit}` and `related_blocks {block_uuid}`: authenticated, audited, error when disabled for the graph; HTTP tests.
- `bitacora-cli semantic status|resync|purge --graph <path>`.

## Notes
Implements BIT-SP-0010.R5, BIT-SP-0010.R6.
