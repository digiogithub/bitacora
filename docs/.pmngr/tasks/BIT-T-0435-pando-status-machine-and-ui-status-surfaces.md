---
id: BIT-T-0435
type: task
title: Pando status machine and UI status surfaces
status: backlog
priority: medium
parent: BIT-US-0140
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-pando, bitacora-app]
estimate: 3
created: 2026-10-07T09:16:34Z
updated: 2026-10-07T09:16:34Z
---

## Description
Health-check loop with backoff, states Disabled/NotConfigured/Connecting/Ok/Unauthorized/Unreachable/TooOld; sidebar footer dot, top-bar chip, settings banner; features query status before acting.

## Acceptance Criteria
- Tests with mock server toggling availability; UI stays responsive with Pando stopped.
