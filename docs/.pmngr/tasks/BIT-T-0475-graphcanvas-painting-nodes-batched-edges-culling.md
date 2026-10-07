---
id: BIT-T-0475
type: task
title: "GraphCanvas painting: nodes, batched edges, culling"
status: in_progress
priority: high
parent: BIT-US-0157
milestone: BIT-M-0009
author: mcp
labels: [v2, graph, bitacora-app]
estimate: 3
created: 2026-10-07T09:20:34Z
updated: 2026-10-07T10:44:11Z
started: 2026-10-07T10:44:11Z
---

## Description
`views/graph/canvas.rs`: world→screen transform, node quads with token colours, edges batched per colour into a single path, viewport culling, cached path while paused; graph view route from sidebar.

## Acceptance Criteria
- `#[gpui::test]` renders fixture graph without panic; manual fps check at 3k nodes.
