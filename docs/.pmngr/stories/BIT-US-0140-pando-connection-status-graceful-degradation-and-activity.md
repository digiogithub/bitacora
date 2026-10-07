---
id: BIT-US-0140
type: story
title: Pando connection status, graceful degradation and activity log
status: in_review
priority: medium
parent: BIT-EP-0020
milestone: BIT-M-0007
author: mcp
labels: [v2, pando, bitacora-pando, bitacora-app]
estimate: 5
created: 2026-10-07T09:15:49Z
updated: 2026-10-07T12:25:49Z
started: 2026-10-07T12:17:48Z
---

## Description
As a user, I want to always know whether Pando is available and what was sent to it.

## Acceptance Criteria
- Status machine: Disabled / NotConfigured / Connecting / Ok / Unauthorized / Unreachable / TooOld, with backoff health checks; shown in sidebar footer, top-bar chip and settings.
- Every feature degrades to "unavailable" without blocking the UI; lexical search, editing, sync and MCP unaffected (test with Pando stopped).
- Machine-local, bounded, clearable Pando activity log (sync batches by count/ids, runs, MCP calls with the `pando` token, approvals) with a viewer.

## Notes
Implements BIT-SP-0009.R4, BIT-SP-0009.R7.
