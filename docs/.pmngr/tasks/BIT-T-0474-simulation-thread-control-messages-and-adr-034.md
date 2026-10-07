---
id: BIT-T-0474
type: task
title: Simulation thread, control messages and ADR-034
status: backlog
priority: high
parent: BIT-US-0156
milestone: BIT-M-0009
author: mcp
labels: [v2, graph, bitacora-graph, docs]
estimate: 2
created: 2026-10-07T09:20:34Z
updated: 2026-10-07T09:20:34Z
---

## Description
Background thread running ticks, accepting messages (params, pause, reheat, drag/pin, replace graph keeping positions) and publishing snapshots; ADR-034 and dependency-direction update.

## Acceptance Criteria
- Tests for message handling and settle detection; bench 5k nodes ≥ 60 ticks/s.
