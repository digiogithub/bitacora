---
id: BIT-T-0444
type: task
title: Cold-start reconcile and initial full index
status: backlog
priority: high
parent: BIT-US-0143
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-pando]
estimate: 2
created: 2026-10-07T09:17:36Z
updated: 2026-10-07T09:17:36Z
---

## Description
On enable/start: diff local eligible blocks against `semantic_state` (and the remote prefix listing when available) to enqueue missing upserts and orphan deletes; throttle initial indexing of large graphs; journals policy setting (all / last N months).

## Acceptance Criteria
- Restart with no changes sends nothing; large-preset initial index measured and recorded.
