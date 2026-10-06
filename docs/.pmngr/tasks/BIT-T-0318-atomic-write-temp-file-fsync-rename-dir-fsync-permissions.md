---
id: BIT-T-0318
type: task
title: "atomic_write: temp file, fsync, rename, dir fsync, permissions, in-place fallback"
status: backlog
priority: critical
parent: BIT-US-0064
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, io]
estimate: 3
created: 2026-10-06T14:33:18Z
updated: 2026-10-06T14:33:18Z
---

## Description
`crates/bitacora-core/src/writer/atomic.rs`: `atomic_write(path, bytes) -> Result<WriteInfo, WriteError>`: create `.<name>.bitacora-tmp` with `O_EXCL` in the same dir (remove stale first), write all, `sync_all`, copy permissions from the target, `rename` (Windows: `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH`, retry on sharing violations up to 5× 50 ms), then fsync the directory on Unix. On `EXDEV`/unsupported rename → in-place write with warning. Return new `mtime`/`len`. Also `cleanup_stale_tmp(graph_root)` at graph open (skips ignored dirs).

## Acceptance Criteria
- Unit tests: new file, overwrite, mode preserved (Unix), stale temp removed, fallback path exercised via a fault-injection trait.
- Temp file never left behind on success or on error.

## Notes
Story BIT-US-0064. Implements BIT-SP-0005.R5. ADR-011. AGENTS.md rule 4.
