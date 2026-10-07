---
id: BIT-T-0472
type: task
title: Quadtree and Barnes-Hut many-body force
status: backlog
priority: high
parent: BIT-US-0156
milestone: BIT-M-0009
author: mcp
labels: [v2, graph, bitacora-graph]
estimate: 3
created: 2026-10-07T09:20:34Z
updated: 2026-10-07T09:20:34Z
---

## Description
Create `crates/bitacora-graph`; quadtree with centre-of-mass aggregation, Barnes-Hut approximation (theta 0.5, distance min 1, max = charge range).

## Acceptance Criteria
- Unit tests vs brute-force within tolerance; criterion bench.
