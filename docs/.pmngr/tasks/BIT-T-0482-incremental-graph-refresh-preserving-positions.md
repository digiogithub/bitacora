---
id: BIT-T-0482
type: task
title: Incremental graph refresh preserving positions
status: backlog
priority: medium
parent: BIT-US-0159
milestone: BIT-M-0009
author: mcp
labels: [v2, graph]
estimate: 2
created: 2026-10-07T09:20:35Z
updated: 2026-10-07T09:20:35Z
---

## Description
Subscribe to index changes, recompute `GraphData` off-thread, diff by node id, keep positions, seed new nodes near a neighbour, reheat ~0.3.

## Acceptance Criteria
- Test: surviving nodes move less than a tolerance after adding one link.
