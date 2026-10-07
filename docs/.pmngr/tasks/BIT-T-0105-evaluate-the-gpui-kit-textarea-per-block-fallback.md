---
id: BIT-T-0105
type: task
title: Evaluate the GPUI Kit Textarea-per-block fallback
status: done
priority: high
parent: BIT-US-0072
milestone: BIT-M-0001
author: mcp
labels: [spike, block-editor, bitacora-app]
estimate: 2
created: 2026-10-06T14:29:45Z
updated: 2026-10-07T08:21:12Z
started: 2026-10-06T17:59:48Z
closed: 2026-10-07T08:21:12Z
---

## Description
Build a minimal alternative spike page where the editing block is a GPUI Kit `Textarea` (auto-grow, inline atomic tokens for `[[refs]]`) instead of the custom element, behind `--spike-editor=textarea`. Check whether edge-key propagation (Up/Down on first/last line, Enter, Backspace at 0, Tab) can be intercepted to implement cross-block navigation, how inline styling and caret anchoring for popups (`bounds_for_range`) work, and IME behaviour on one OS. Compare effort/feasibility against Option C.

## Acceptance Criteria
- A comparison table (capability x Option A vs Option C: IME, styling, edge keys, popups anchoring, perf, API churn exposure) ready for the spike report.

## Notes
- [[gpui-and-gpui-kit]] §3.1 Option A, §2.2 (Textarea), Risk R3 ("fall back to Textarea per block for the MVP"). ADR-002.
