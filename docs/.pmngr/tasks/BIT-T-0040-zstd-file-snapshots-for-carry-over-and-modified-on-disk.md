---
id: BIT-T-0040
type: task
title: zstd file snapshots for carry-over and modified-on-disk diffs
status: cancelled
priority: medium
parent: BIT-US-0006
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 2
created: 2026-10-06T14:27:35Z
updated: 2026-10-06T15:11:55Z
closed: 2026-10-06T15:11:55Z
---

## Description
Cancelled by ADR-017: no `file_snapshots` table and no on-disk copy of file content in the index.

Original scope (kept for reference, do not implement): store `zstd(bytes)` (level 3) in `file_snapshots` at step 11 of the replace transaction; expose `IndexReader::snapshot(path) -> Option<Vec<u8>>` for the write-safety layer (BIT-EP-0008) and the carry-over similarity check; setting `index.snapshots`.

Replacements:
- Merge base for external edits: kept in memory only in `bitacora-core` (last bytes read/written per loaded/touched file, `HashMap<FileId, Arc<[u8]>>` / `DiskSnapshot`). After a restart there is no base: no pending local edits → plain reload; pending edits → 2-way per-block diff in the conflict notice (BIT-US-0069, BIT-US-0070).
- UUID carry-over similarity check: uses the old `blocks.content` rows read before the per-file replace (BIT-T-0039).

## Acceptance Criteria
- None (cancelled).

## Notes
BIT-SP-0003.R12 (updated). ADR-017. Open question 2 in [[sqlite-index-schema]] §10 resolved.
