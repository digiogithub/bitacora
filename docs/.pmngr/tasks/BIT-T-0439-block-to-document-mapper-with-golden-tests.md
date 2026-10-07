---
id: BIT-T-0439
type: task
title: Block-to-document mapper with golden tests
status: backlog
priority: high
parent: BIT-US-0142
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-pando, tests]
estimate: 2
created: 2026-10-07T09:17:36Z
updated: 2026-10-07T09:17:36Z
---

## Description
Build document text (title + breadcrumb + content, properties stripped per policy) and metadata from index rows; apply ContentPolicy, min length, property-only and asset-only filters.

## Acceptance Criteria
- Golden snapshot tests (`insta`) over `fixtures/graphs/**`.
