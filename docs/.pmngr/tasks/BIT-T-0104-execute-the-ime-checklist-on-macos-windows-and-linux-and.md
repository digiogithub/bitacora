---
id: BIT-T-0104
type: task
title: Execute the IME checklist on macOS, Windows and Linux and fix blockers
status: backlog
priority: critical
parent: BIT-US-0072
milestone: BIT-M-0001
author: mcp
labels: [spike, ime, bitacora-app]
estimate: 5
created: 2026-10-06T14:29:45Z
updated: 2026-10-06T14:29:45Z
---

## Description
Run the checklist against the CI-built spike artifacts on each platform; record results (and screenshots for failures) in the checklist's results table. Fix spike-level bugs that block evaluation (e.g. wrong `bounds_for_range` row, marked range not cleared on blur). For failures rooted in GPUI/the `gpui-pre` snapshot, reproduce with GPUI's `examples/input.rs` and file/link upstream issues; record them as risks rather than workarounds.

## Acceptance Criteria
- Results table complete for all rows; every failure is classified as "spike bug (fixed)", "GPUI upstream (issue link)" or "platform limitation".
- No open spike bug remains among fixable failures.

## Notes
- [[gpui-and-gpui-kit]] §1.2 (Linux text-input-v3/XIM fragility), §1.5, Risk R4; [[block-editor]] §7.2.
