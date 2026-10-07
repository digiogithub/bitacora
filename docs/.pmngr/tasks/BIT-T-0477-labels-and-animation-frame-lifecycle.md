---
id: BIT-T-0477
type: task
title: Labels and animation-frame lifecycle
status: in_progress
priority: medium
parent: BIT-US-0157
milestone: BIT-M-0009
author: mcp
labels: [v2, graph, bitacora-app]
estimate: 2
created: 2026-10-07T09:20:34Z
updated: 2026-10-07T10:44:11Z
started: 2026-10-07T10:44:11Z
---

## Description
Labels shaped only above a zoom threshold or on hover/current; frame requests only while the simulation is alive or the user interacts.

## Acceptance Criteria
- Test: settled + idle → no frame requests.
