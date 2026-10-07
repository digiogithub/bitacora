---
id: BIT-T-0406
type: task
title: Responsive breakpoints, focus order and reduce-motion
status: in_review
priority: low
parent: BIT-US-0128
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, a11y]
estimate: 3
created: 2026-10-07T09:13:46Z
updated: 2026-10-07T11:52:48Z
started: 2026-10-07T11:52:48Z
---

## Description
Right panel overlays below ~1170px, sidebar auto-collapses below a threshold, consistent tab order, focus rings, reduce-motion honoured by panel/popover transitions.

## Acceptance Criteria
- `#[gpui::test]` at 640/1000/1400px; focus order test.
