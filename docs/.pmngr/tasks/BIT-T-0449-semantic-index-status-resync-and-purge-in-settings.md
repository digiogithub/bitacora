---
id: BIT-T-0449
type: task
title: Semantic index status, resync and purge in settings
status: backlog
priority: medium
parent: BIT-US-0145
milestone: BIT-M-0007
author: mcp
labels: [v2, settings, bitacora-app]
estimate: 1
created: 2026-10-07T09:17:36Z
updated: 2026-10-07T09:54:52Z
---

## Description
Per-graph status in settings: synced, pending, last error and last sync. Resync and Purge buttons; Purge deletes every document id recorded in the ledger, one call per document with bounded concurrency, then clears the ledger.

## Acceptance Criteria
- After a purge, a search with `path_prefix = bitacora/<graph_id>/` returns nothing (mock verification).
