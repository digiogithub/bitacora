---
id: BIT-T-0473
type: task
title: Link, collide and gravity forces with alpha cooling
status: done
priority: high
parent: BIT-US-0156
milestone: BIT-M-0009
author: mcp
labels: [v2, graph, bitacora-graph]
estimate: 3
created: 2026-10-07T09:20:34Z
updated: 2026-10-07T10:15:46Z
closed: 2026-10-07T10:15:46Z
---

## Description
Velocity-Verlet integrator with link spring, collision, x/y gravity, centring, alpha decay/min/target, velocity decay, pinned nodes; deterministic seeded initial placement.

## Acceptance Criteria
- Property tests (finite positions, energy decreasing); deterministic output per seed.
