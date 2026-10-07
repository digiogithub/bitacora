---
id: BIT-US-0121
type: story
title: Window resize edges, tiling insets and per-OS checklist
status: in_review
priority: medium
parent: BIT-EP-0016
milestone: BIT-M-0006
author: mcp
labels: [v2, frameless, bitacora-app, docs]
estimate: 3
created: 2026-10-07T09:12:22Z
updated: 2026-10-07T10:39:17Z
started: 2026-10-07T10:39:17Z
---

## Description
As a user, I want to resize the frameless window from any edge and have it behave when tiled or maximized.

## Acceptance Criteria
- Resize from 8 edges/corners on Linux CSD (`window_border`, `start_window_resize`), native on Windows/macOS; correct insets/shadow when tiled or maximized (`set_client_inset`); minimum window size enforced.
- Manual checklist `docs/design/frameless-checklist.md` run and recorded per OS; ADR-033 added.

## Notes
Implements BIT-SP-0008.R5.
