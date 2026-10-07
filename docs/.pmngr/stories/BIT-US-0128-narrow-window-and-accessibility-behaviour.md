---
id: BIT-US-0128
type: story
title: Narrow-window and accessibility behaviour
status: in_review
priority: low
parent: BIT-EP-0017
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, a11y, bitacora-app]
estimate: 3
created: 2026-10-07T09:13:09Z
updated: 2026-10-07T11:52:48Z
started: 2026-10-07T11:52:48Z
---

## Description
As a user on a small screen, I want the layout to adapt: right panel overlays below ~1170px, sidebar collapses below a threshold; focus order and reduce-motion respected everywhere.

## Acceptance Criteria
- `#[gpui::test]` at 640 and 1000px widths; tab order test across top bar, sidebar and editor.

## Notes
Implements BIT-SP-0008.R6.
