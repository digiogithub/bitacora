---
id: BIT-T-0152
type: task
title: Per-user instance lock shared by app and CLI
status: backlog
priority: high
parent: BIT-US-0086
milestone: BIT-M-0005
author: mcp
labels: [release, bitacora-core]
estimate: 2
created: 2026-10-06T14:30:21Z
updated: 2026-10-06T14:30:21Z
---

## Description
Add `bitacora_core::instance` (sync, no tokio): `InstanceLock::acquire(data_dir, kind: InstanceKind::{Desktop, Headless}) -> Result<InstanceLock, AlreadyRunning { kind, pid, mcp_port }>` using an OS advisory file lock (`fs4` or `std::fs::File::try_lock` if available on the MSRV) on `<data_dir>/instance.lock`, with a small JSON sidecar `instance.json` (`pid`, `kind`, `mcp_port`, `ipc_endpoint`, `started_at`) written atomically after acquisition. Lock is released on drop and by the OS on crash, so stale sidecars are detected (lock free -> overwrite).

## Acceptance Criteria
- Tests: second acquire in another process (spawned test helper binary) fails with the first holder's info; acquire succeeds after the holder is killed (stale recovery).
- Works on Linux, macOS, Windows CI legs.

## Notes
- [[mcp-server]] §2; AGENTS.md §3 rule 3 (single writer), rule 4 (atomic writes for the sidecar). ADR-012 (core stays sync).
