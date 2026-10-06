---
id: BIT-US-0062
type: story
title: Single-writer command queue for UI, MCP, sync and watcher mutations
status: done
priority: critical
parent: BIT-EP-0008
milestone: BIT-M-0003
author: mcp
labels: [core, io, bitacora-core]
estimate: 5
created: 2026-10-06T14:29:01Z
updated: 2026-10-06T18:39:25Z
started: 2026-10-06T18:28:32Z
closed: 2026-10-06T18:39:25Z
---

## Description
As an AI agent writing through MCP while a user types in the app, I want all mutations to be serialized through one command queue, so that concurrent edits never interleave inconsistently and every change gets undo, echo suppression and indexing.

The queue lives in `bitacora-core` (synchronous, executor-agnostic) and owns the `Graph`; producers (`bitacora-app`, `bitacora-mcp`, `bitacora-sync`, `bitacora-watch`) submit `Command`s tagged with their origin and receive a result handle.

## Acceptance Criteria
- `CommandQueue` with a single consumer thread that owns `Graph` and applies commands in submission order.
- `Command { origin: Ui|Mcp|Sync|External, cmd, reply }`; results delivered via a oneshot-style channel usable from sync and async code.
- Read snapshots for views/MCP without blocking the writer (e.g. `Arc` page snapshots or a read lock with short critical sections).
- A guard (clippy `disallowed-methods` config or test) prevents direct `std::fs::write` to graph paths outside the core writer.
- Stress test: 2 producers × 1,000 commands → all applied, final state deterministic for a given order.

## Notes
Implements: BIT-SP-0005.R1.
See [[architecture]] §3 key rules, AGENTS.md rule 3, [[block-editor]] §3.3. ADR-011, ADR-012.
