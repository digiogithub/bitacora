---
id: BIT-US-0156
type: story
title: Force layout engine in bitacora-graph
status: done
priority: high
parent: BIT-EP-0024
milestone: BIT-M-0009
author: mcp
labels: [v2, graph, bitacora-graph]
estimate: 8
created: 2026-10-07T09:19:46Z
updated: 2026-10-07T10:15:46Z
closed: 2026-10-07T10:15:46Z
---

## Description
As the graph view, I need a fast, deterministic force-directed layout that runs off the UI thread.

## Acceptance Criteria
- New GPUI-free crate `bitacora-graph`: quadtree Barnes-Hut many-body (theta 0.5, distance max = charge range), link spring, collide (r 26, 2 iterations), x/y gravity 0.02, alpha cooling, velocity decay 0.5; Logseq defaults (link 70, charge -600, range 600).
- Background simulation thread with parameter messages (forces, pause, reheat, pin/drag node) and position snapshots (`Arc<[[f32;2]]>`).
- Deterministic seed; property tests (finite positions, energy decreases); bench: 5k nodes ≥ 60 ticks/s; ADR-034; dependency-direction docs updated.

## Notes
Implements BIT-SP-0012.R3. Logseq params: `extensions/graph/pixi.cljs:59-100` (behaviour only).
