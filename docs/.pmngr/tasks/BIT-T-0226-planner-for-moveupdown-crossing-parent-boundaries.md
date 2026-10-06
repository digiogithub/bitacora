---
id: BIT-T-0226
type: task
title: Planner for MoveUpDown crossing parent boundaries
status: backlog
priority: critical
parent: BIT-US-0034
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, outliner]
estimate: 2
created: 2026-10-06T14:31:33Z
updated: 2026-10-06T14:31:33Z
---

## Description
In `crates/bitacora-core/src/commands/move_up_down.rs` implement `plan_move_up_down(ids, up)`: swap the consecutive top-level range with the previous/next sibling; at the boundary, move into the previous parent's last-child position (up) / next parent's first-child position (down), following `move-blocks-up-down` (`core.cljs:760-800`). No-op at page edges.

## Acceptance Criteria
- Op tests: swap, cross-boundary up/down, page edges, multi-block range.
- Clean moved blocks remain clean when depth is unchanged.

## Notes
Story BIT-US-0034. Implements BIT-SP-0004.R10.
