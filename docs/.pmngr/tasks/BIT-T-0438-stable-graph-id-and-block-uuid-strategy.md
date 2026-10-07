---
id: BIT-T-0438
type: task
title: Stable graph_id and block uuid strategy
status: backlog
priority: high
parent: BIT-US-0142
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-index]
estimate: 2
created: 2026-10-07T09:17:36Z
updated: 2026-10-07T09:17:36Z
---

## Description
Define `graph_id` (stored in index `meta`, stable across index rebuilds — e.g. derived from a machine-local graph registry entry) and the identity of blocks without `id::` (index uuid stability across rebuilds or fallback to content-anchored ids). Never write `id::` to user files for this purpose.

## Acceptance Criteria
- Test: rebuild-from-scratch yields the same document ids for unchanged blocks (or the documented fallback reconciles without re-embedding).
