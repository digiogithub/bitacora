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
updated: 2026-10-07T09:17:36Z
---

## Description
Per-graph status (synced/pending/last error/last sync), Resync and Purge buttons (purge uses delete-by-prefix or per-document fallback).

## Acceptance Criteria
- Purge leaves no documents under the prefix (mock verification).
