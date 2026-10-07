---
id: BIT-T-0443
type: task
title: Batch sender with debounce, backoff and capability fallback
status: backlog
priority: high
parent: BIT-US-0143
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-pando]
estimate: 3
created: 2026-10-07T09:17:36Z
updated: 2026-10-07T09:17:36Z
---

## Description
Async sender draining the outbox in batches (batch endpoint or per-document fallback, `index_only` + `content_hash` when supported), exponential backoff, pause when offline/unauthorized, progress events.

## Acceptance Criteria
- Mock-Pando tests: offline period converges; 401 pauses without data loss.
