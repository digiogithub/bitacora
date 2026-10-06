---
id: BIT-T-0181
type: task
title: Status bar indexing progress indicator
status: in_progress
priority: medium
parent: BIT-US-0073
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, ui]
estimate: 1
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T18:28:35Z
started: 2026-10-06T18:28:35Z
---

## Description
Add an indexing segment to the `StatusBar` (from BIT-US-0025): GPUI Kit `Progress` + label "Indexing 1,234 / 5,000" fed by `IndexEvent::Progress`, hidden when done; clicking it opens a popover with diagnostics count linking to a diagnostics list (read-only).

## Acceptance Criteria
- `#[gpui::test]`: progress events update the label; final event hides the segment.

## Notes
BIT-SP-0003.R6, BIT-SP-0003.R11.
