---
id: BIT-T-0429
type: task
title: Top-bar AI/Pando button and quick settings popover
status: backlog
priority: medium
parent: BIT-US-0137
milestone: BIT-M-0007
author: mcp
labels: [v2, settings, bitacora-app]
estimate: 2
created: 2026-10-07T09:16:33Z
updated: 2026-10-07T09:16:33Z
---

## Description
Wire the top-bar AI button: popover with status chip, quick feature toggles for the current graph and "Open Pando settings"; first-run state guides to setup.

## Acceptance Criteria
- `#[gpui::test]`: button opens popover; disabled state when Pando off.
