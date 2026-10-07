---
id: BIT-T-0470
type: task
title: Global graph query with nodes, edges and degree
status: backlog
priority: high
parent: BIT-US-0155
milestone: BIT-M-0009
author: mcp
labels: [v2, graph, bitacora-index]
estimate: 3
created: 2026-10-07T09:20:34Z
updated: 2026-10-07T09:20:34Z
---

## Description
SQL over `block_page_refs` ⨝ `blocks` (kinds 1,2,3,8), UNION `page_tags` and namespace parent edges; degree computed; placeholder pages; exclusions of self-links and UUID/asset-like names.

## Acceptance Criteria
- Fixture tests with expected nodes/edges; large-preset timing recorded.
