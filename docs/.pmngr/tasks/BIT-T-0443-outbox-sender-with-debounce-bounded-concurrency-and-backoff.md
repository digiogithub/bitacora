---
id: BIT-T-0443
type: task
title: Outbox sender with debounce, bounded concurrency and backoff
status: done
priority: high
parent: BIT-US-0143
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-pando]
estimate: 3
created: 2026-10-07T09:17:36Z
updated: 2026-10-07T10:50:56Z
closed: 2026-10-07T10:50:56Z
---

## Description
Async sender draining the outbox with per-document REST upsert and delete calls:
- bounded concurrency (configurable, default 4) and debounce;
- exponential backoff, pausing when Pando is offline or returns unauthorized;
- progress events, and a rate limit for the initial full index.

## Acceptance Criteria
- Mock-Pando tests: an offline period converges, and a 401 pauses without data loss.
- Throughput on the large preset is measured and recorded.
