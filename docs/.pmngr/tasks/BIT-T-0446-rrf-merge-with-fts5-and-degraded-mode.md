---
id: BIT-T-0446
type: task
title: RRF merge with FTS5 and degraded mode
status: backlog
priority: high
parent: BIT-US-0144
milestone: BIT-M-0007
author: mcp
labels: [v2, search, bitacora-index]
estimate: 3
created: 2026-10-07T09:17:36Z
updated: 2026-10-07T09:17:36Z
---

## Description
Hybrid search API combining FTS5 and semantic lists via existing RRF (k=60), timeout budget for the semantic leg, `semantic_unavailable` flag on failure.

## Acceptance Criteria
- Tests: semantic-only match surfaces; Pando down → identical lexical results + flag; latency budget respected.
