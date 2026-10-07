---
id: BIT-T-0442
type: task
title: Index-event subscriber that diffs hashes into the outbox
status: backlog
priority: high
parent: BIT-US-0143
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-pando]
estimate: 3
created: 2026-10-07T09:17:36Z
updated: 2026-10-07T09:17:36Z
---

## Description
Consume `IndexEvent`s, compare block hashes with `semantic_state`, enqueue upserts/deletes (renames → metadata update), honour consent/exclusion changes by enqueuing deletes.

## Acceptance Criteria
- Tests: single edit → one upsert; delete → delete; exclusion added → deletes queued.
