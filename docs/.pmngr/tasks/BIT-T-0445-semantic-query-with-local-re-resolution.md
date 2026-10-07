---
id: BIT-T-0445
type: task
title: Semantic query with local re-resolution
status: done
priority: high
parent: BIT-US-0144
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-pando, search]
estimate: 2
created: 2026-10-07T09:17:36Z
updated: 2026-10-07T11:03:30Z
started: 2026-10-07T11:03:22Z
closed: 2026-10-07T11:03:30Z
---

## Description
Call KB search scoped to the graph prefix, map `file_path` → block uuid, resolve in local index, drop stale, return local content.

## Acceptance Criteria
- Test with mock returning a deleted block → dropped.
