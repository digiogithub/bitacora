---
id: BIT-T-0441
type: task
title: Machine-local semantic sync ledger and outbox
status: backlog
priority: high
parent: BIT-US-0143
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-pando, schema]
estimate: 2
created: 2026-10-07T09:17:36Z
updated: 2026-10-07T09:54:52Z
---

## Description
Create the per-graph ledger DB outside the index (it must survive index rebuilds, because it records what exists remotely), with tables `semantic_state` and `semantic_outbox`, migrations and a schema version. An index rebuild never touches it, and a reconcile diffs the rebuilt index against it.

## Acceptance Criteria
- Migration tests.
- A test proves that after an index rebuild, no document is re-sent and deleted blocks still produce remote deletes.
