---
id: BIT-US-0159
type: story
title: Local graph, focus mode and incremental refresh
status: done
priority: medium
parent: BIT-EP-0024
milestone: BIT-M-0009
author: mcp
labels: [v2, graph, bitacora-app]
estimate: 5
created: 2026-10-07T09:19:46Z
updated: 2026-10-07T11:02:43Z
started: 2026-10-07T11:02:31Z
closed: 2026-10-07T11:02:43Z
---

## Description
As a user, I want a small graph of the current page in the right panel, to focus on parts of the big graph, and to see changes appear without the layout jumping.

## Acceptance Criteria
- Local graph in the Context tab: current page highlighted, updates on navigation; click navigates.
- Modifier-click focuses nodes; N-hops slider limits visible nodes; reset.
- Index change events diff `GraphData`, keep positions of surviving nodes, seed new nodes near a neighbour, reheat alpha ~0.3.

## Notes
Implements BIT-SP-0012.R6.
