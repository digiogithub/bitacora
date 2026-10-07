---
id: BIT-T-0476
type: task
title: Pan, zoom, drag, hover and click-to-open
status: backlog
priority: high
parent: BIT-US-0157
milestone: BIT-M-0009
author: mcp
labels: [v2, graph, bitacora-app]
estimate: 3
created: 2026-10-07T09:20:34Z
updated: 2026-10-07T09:20:34Z
---

## Description
Hitbox + mouse handlers: drag pan, wheel/pinch zoom at cursor, node drag (pin + reheat), hover via spatial grid with neighbour highlight, click threshold vs drag, open page.

## Acceptance Criteria
- `#[gpui::test]` simulated input: click opens page, drag does not.
