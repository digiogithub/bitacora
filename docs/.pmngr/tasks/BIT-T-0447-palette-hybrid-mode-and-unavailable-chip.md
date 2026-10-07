---
id: BIT-T-0447
type: task
title: Palette hybrid mode and unavailable chip
status: backlog
priority: medium
parent: BIT-US-0145
milestone: BIT-M-0007
author: mcp
labels: [v2, search, bitacora-app]
estimate: 2
created: 2026-10-07T09:17:36Z
updated: 2026-10-07T09:17:36Z
---

## Description
Palette uses the hybrid API when available, mode toggle, semantic-sourced results marked subtly, chip when semantic is unavailable.

## Acceptance Criteria
- `#[gpui::test]` with mocked search service in both states.
