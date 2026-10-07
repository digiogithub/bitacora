---
id: BIT-T-0489
type: task
title: "Spike: managed instance on the user's shared KB"
status: done
priority: high
parent: BIT-US-0141
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-pando, managed, spike]
estimate: 2
created: 2026-10-07T09:54:20Z
updated: 2026-10-07T11:03:28Z
started: 2026-10-07T10:35:57Z
closed: 2026-10-07T11:03:28Z
---

## Description
Verify how a managed `pando serve` started from an instance cwd can use the user's shared remembrances/KB store (agent memory) instead of a per-instance `data/` dir:
- which config keys select the KB/DB location;
- whether a second Pando process can safely share the store with the user's own Pando (IPC lock, dbproxy `WriteWithRetry`);
- what happens when the user's Pando is already running: connect to it in external mode instead of spawning.

Record the findings and the decision in `docs/design/pando-integration.md`.

## Acceptance Criteria
- Documented answer with Pando source refs.
- A semantic document upserted by the managed instance is visible from the user's Pando `kb_search_documents` with `path_prefix = bitacora/`.
