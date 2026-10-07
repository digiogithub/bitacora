---
id: BIT-T-0387
type: task
title: Card, Tab, Segmented and Popover shell components
status: backlog
priority: high
parent: BIT-US-0117
milestone: BIT-M-0006
author: mcp
labels: [v2, components]
estimate: 3
created: 2026-10-07T09:12:06Z
updated: 2026-10-07T09:12:06Z
---

## Description
Container components: `Card` (1px line border, radius tokens), `Tab` with underline indicator, `Segmented` control, `Popover` shell (the only shadowed surface) honouring reduce-motion.

## Acceptance Criteria
- `#[gpui::test]` per component; popover closes on Escape and outside click.
