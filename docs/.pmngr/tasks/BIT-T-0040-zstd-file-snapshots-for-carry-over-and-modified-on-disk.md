---
id: BIT-T-0040
type: task
title: zstd file snapshots for carry-over and modified-on-disk diffs
status: backlog
priority: medium
parent: BIT-US-0006
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 2
created: 2026-10-06T14:27:35Z
updated: 2026-10-06T14:27:35Z
---

## Description
Store `zstd(bytes)` (level 3) in `file_snapshots` at step 11 of the replace transaction; expose `IndexReader::snapshot(path) -> Option<Vec<u8>>` for the write-safety layer (BIT-EP-0008) and the carry-over similarity check. Setting `index.snapshots = true` (default); when off, carry-over falls back to hash-only pairing.

## Acceptance Criteria
- Round-trip test: snapshot decompresses to the exact indexed bytes.
- Snapshot removed with the file row (cascade).
- Disabling the setting skips writes and does not break carry-over tests.

## Notes
BIT-SP-0003.R12. Open question 2 in [[sqlite-index-schema]] §10.
