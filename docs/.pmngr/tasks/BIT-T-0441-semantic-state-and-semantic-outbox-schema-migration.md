---
id: BIT-T-0441
type: task
title: semantic_state and semantic_outbox schema migration
status: backlog
priority: high
parent: BIT-US-0143
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-index, schema]
estimate: 2
created: 2026-10-07T09:17:36Z
updated: 2026-10-07T09:17:36Z
---

## Description
Add tables with migration and schema version bump in `bitacora-index`; define rebuild semantics (state cleared → reconcile; pending deletes re-derived from remote list or local state).

## Acceptance Criteria
- Migration test from v1 schema; rebuild test.
