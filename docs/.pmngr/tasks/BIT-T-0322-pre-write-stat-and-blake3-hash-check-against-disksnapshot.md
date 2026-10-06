---
id: BIT-T-0322
type: task
title: Pre-write stat and blake3 hash check against DiskSnapshot
status: done
priority: critical
parent: BIT-US-0065
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, io]
estimate: 2
created: 2026-10-06T14:33:18Z
updated: 2026-10-06T18:52:31Z
closed: 2026-10-06T18:52:31Z
---

## Description
`crates/bitacora-core/src/writer/precheck.rs`: `precheck(path, &DiskSnapshot) -> PreCheck::{Unchanged, Changed { bytes, hash, mtime }, Missing, NotAFile}`: `stat`; if `(len, mtime)` equal → Unchanged; else read + `blake3` → equal hash → Unchanged (update mtime in snapshot); different → Changed. The writer only proceeds on Unchanged; Changed submits an `External` command carrying the new bytes to the queue (merge/conflict path).

## Acceptance Criteria
- Temp-dir tests: unchanged, touch-only, modified, deleted, replaced by directory.
- A Changed result never results in a write in the same cycle.

## Notes
Story BIT-US-0065. Implements BIT-SP-0005.R4. ADR-011.
