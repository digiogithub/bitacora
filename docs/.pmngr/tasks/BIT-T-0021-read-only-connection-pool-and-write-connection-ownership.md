---
id: BIT-T-0021
type: task
title: Read-only connection pool and write connection ownership
status: done
priority: high
parent: BIT-US-0004
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 2
created: 2026-10-06T14:26:33Z
updated: 2026-10-06T17:14:37Z
started: 2026-10-06T17:09:56Z
closed: 2026-10-06T17:14:37Z
---

## Description
`crates/bitacora-index/src/pool.rs`: a small fixed-size pool (default `min(4, num_cpus)`) of read-only connections opened with `SQLITE_OPEN_READ_ONLY` and the same pragmas, handing out guards (`ReadConn`). The single write connection is moved into the writer thread (see writer story) and is never exposed publicly. `Index` is `Send + Sync + Clone` (Arc inside) so `bitacora-app` and `bitacora-mcp` share it. Keep the crate synchronous (no tokio).

## Acceptance Criteria
- Test: concurrent readers on 4 threads run while a write transaction is open (WAL) without `SQLITE_BUSY`.
- Attempting a write through a `ReadConn` fails with `SQLITE_READONLY`.
- Public API exposes no `rusqlite` types.

## Notes
BIT-SP-0003.R7. ADR-004, ADR-012.
