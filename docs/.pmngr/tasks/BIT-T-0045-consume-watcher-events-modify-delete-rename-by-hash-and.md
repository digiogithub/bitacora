---
id: BIT-T-0045
type: task
title: "Consume watcher events: modify, delete, rename-by-hash and overflow"
status: backlog
priority: high
parent: BIT-US-0007
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 2
created: 2026-10-06T14:27:35Z
updated: 2026-10-06T14:27:35Z
---

## Description
`crates/bitacora-index/src/pipeline/events.rs`: adapter from `bitacora-watch` debounced events (debounce, 500 ms unlink re-check and echo suppression are implemented in `bitacora-watch`, BIT-EP-0008) to pipeline jobs: Create/Modify → change filter; Remove → `Delete`; Rename(from,to) → if blake3(to) == hash(from) then `UPDATE files SET path` + reparse with carry-over from the old rows, else delete+create; case-only rename updated in place; Overflow / `MustScanSubDirs` / root removed → mark stale and run full reconcile. Also accept `Indexer::index_bytes(path, bytes)` from the core writer for own writes (no wait for the watcher).

## Acceptance Criteria
- Test: rename with identical bytes keeps all block UUIDs.
- Test: overflow event triggers exactly one reconcile.
- Test: own-write path indexes in-memory bytes; the later watcher event is a metadata-only no-op.

## Notes
BIT-SP-0003.R6, BIT-SP-0003.R12. [[sqlite-index-schema]] §4.2, §4.5.
