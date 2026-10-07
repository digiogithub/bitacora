---
id: BIT-T-0385
type: task
title: Buttons, IconButton and icon set (Lucide + AI sparkle)
status: backlog
priority: high
parent: BIT-US-0117
milestone: BIT-M-0006
author: mcp
labels: [v2, components]
estimate: 3
created: 2026-10-07T09:12:05Z
updated: 2026-10-07T09:12:05Z
---

## Description
`Button` variants primary/secondary/ghost/AI, `IconButton` (15/17/18px icons, 1.7 stroke, 30-36px targets), embedded Lucide SVG subset used by the mockups plus the custom 4-point sparkle.

## Acceptance Criteria
- `#[gpui::test]` per variant (size, disabled, focus ring); icons load from embedded assets.
