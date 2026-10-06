---
id: BIT-US-0064
type: story
title: Atomic, crash-safe file writer with serializer self-check
status: done
priority: critical
parent: BIT-EP-0008
milestone: BIT-M-0003
author: mcp
labels: [core, io, bitacora-core]
estimate: 8
created: 2026-10-06T14:29:01Z
updated: 2026-10-06T18:52:32Z
closed: 2026-10-06T18:52:32Z
---

## Description
As a user whose notes are precious, I want every write to be atomic and verified, so that a crash, power loss or serializer bug can never leave a truncated or mis-structured page file.

## Acceptance Criteria
- `atomic_write(path, bytes)`: `.<name>.bitacora-tmp` in the same dir → write → `fsync` → `rename` → dir `fsync` (Unix); permissions preserved; in-place fallback on rename-unsupported filesystems with a warning; Windows uses `ReplaceFileW`/`MoveFileExW(MOVEFILE_REPLACE_EXISTING)` semantics.
- Stale `*.bitacora-tmp` files are removed at graph open.
- Self-check: re-parse serialized bytes and compare depths+texts with the model; on mismatch canonical render + bug log; refuse to write if still mismatched.
- Encoding: UTF-8; new files LF, no BOM, tab indent; edited files keep line ending, BOM and final-newline state.
- Crash-safety test: kill a child process mid-write many times; target is always old or new content.

## Notes
Implements: BIT-SP-0005.R3, BIT-SP-0005.R5, BIT-SP-0005.R6.
See [[block-editor]] §5.1–5.2, [[01-file-graph-layout]] §8, [[04-editor-outliner-operations]] §5 (Logseq `writeFileSync` is not atomic). ADR-011, ADR-003.
