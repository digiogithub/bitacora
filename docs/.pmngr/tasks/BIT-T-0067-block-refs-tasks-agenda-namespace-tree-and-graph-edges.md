---
id: BIT-T-0067
type: task
title: Block refs, tasks/agenda, namespace tree and graph edges queries
status: done
priority: medium
parent: BIT-US-0008
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 3
created: 2026-10-06T14:28:35Z
updated: 2026-10-06T18:39:52Z
closed: 2026-10-06T18:39:52Z
---

## Description
`crates/bitacora-index/src/read/misc.rs`: `block_ref_count(uuid)`, `block_referrers(uuid)`, `resolve_block_ref(uuid)` (may be `None`), `tasks(filter{markers, priority, page})`, `agenda(today, days_ahead)` (scheduled/deadline window, repeated, excluding DONE/CANCELED/CANCELLED), `namespace_children(page)`, `namespace_tree(page)` (recursive CTE), `graph_edges(opts{journals, orphans, builtins})` from direct refs plus namespace edges, honouring `exclude-from-graph-view:: true`. All return domain structs.

## Acceptance Criteria
- Unit tests per function on a small hand-built fixture (dangling block ref → `None`; agenda excludes DONE; namespace tree depth 3).
- Functions are reused by `bitacora-mcp` read tools (no duplicate SQL there).

## Notes
BIT-SP-0003.R8, BIT-SP-0003.R4. [[sqlite-index-schema]] §5.
