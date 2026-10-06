---
id: BIT-T-0044
type: task
title: Cold build fast path (triggers off, batched tx, FTS rebuild, ANALYZE)
status: done
priority: high
parent: BIT-US-0007
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index, performance]
estimate: 3
created: 2026-10-06T14:27:35Z
updated: 2026-10-06T17:53:44Z
closed: 2026-10-06T17:53:44Z
---

## Description
`crates/bitacora-index/src/pipeline/cold.rs`: when the DB is empty (or `FullReparse`): drop FTS triggers, `PRAGMA foreign_keys=OFF`, apply writer jobs in batches of ~200 files per transaction, then `INSERT INTO blocks_fts(blocks_fts) VALUES('rebuild')` (same for `blocks_fts_tri`, `pages_fts`), recreate triggers from `schema/v1_triggers.sql`, `foreign_keys=ON`, `PRAGMA foreign_key_check` (any row → error + full rebuild), `ANALYZE`. Set `meta.last_full_scan_at`. `FtsRebuild` outcome recomputes `search_text` then runs only the FTS rebuild.

## Acceptance Criteria
- Test: cold-built DB answers FTS queries identically to an incrementally built DB.
- Test: `foreign_key_check` empty after cold build of fixtures.

## Notes
BIT-SP-0003.R16. [[sqlite-index-schema]] §4.1 step 6.
