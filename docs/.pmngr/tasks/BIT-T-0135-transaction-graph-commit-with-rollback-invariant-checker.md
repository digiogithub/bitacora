---
id: BIT-T-0135
type: task
title: Transaction, Graph::commit with rollback, invariant checker and Cmd/plan/Refusal skeleton
status: done
priority: critical
parent: BIT-US-0029
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, core]
estimate: 3
created: 2026-10-06T14:30:00Z
updated: 2026-10-06T18:39:25Z
started: 2026-10-06T18:28:39Z
closed: 2026-10-06T18:39:25Z
---

## Description
In `crates/bitacora-core/src/tx.rs` add `Transaction { id, label, ops, cursor_before, cursor_after, pages, at, coalesce }`, `CursorState`, and `Graph::commit(cmd)`: flush hook → `plan(&Graph, Cmd) -> Result<Vec<Op>, Refusal>` → apply ops in order, rolling back the applied prefix (apply inverses in reverse) on error → push to history → mark pages dirty → emit `GraphEvent::Changed { pages, blocks }` for index and views. `check_invariants(&Graph)` (acyclic, single parent slot, unique uuids per graph) runs after every commit in debug builds, cheap version (touched blocks only) in release. Define `Cmd` enum with the commands from [[block-editor]] §3.2 (planners implemented in later stories).

## Acceptance Criteria
- Failing second op leaves the graph identical to before the commit (tested).
- Invariant violations in debug panic in tests with a clear message; release returns `CommitError::Invariant` and rolls back.
- `Refusal` carries a human-readable reason and produces no history entry.

## Notes
Story BIT-US-0029. Implements BIT-SP-0004.R5. ADR-012 (no tokio in core).
