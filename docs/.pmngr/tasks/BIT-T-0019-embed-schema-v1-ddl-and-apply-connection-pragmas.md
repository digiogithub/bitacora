---
id: BIT-T-0019
type: task
title: Embed schema v1 DDL and apply connection pragmas
status: done
priority: critical
parent: BIT-US-0004
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 3
created: 2026-10-06T14:26:33Z
updated: 2026-10-06T17:14:37Z
started: 2026-10-06T17:09:56Z
closed: 2026-10-06T17:14:37Z
---

## Description
Add `crates/bitacora-index/schema/v1.sql` with the full DDL of [[sqlite-index-schema]] §3 (tables `meta`, `files`, `pages`, `page_aliases`, `page_tags`, `blocks`, `block_page_refs`, `block_block_refs`, `block_properties`, `block_property_values`, `diagnostics`; FTS5 `blocks_fts`, `blocks_fts_tri`, `pages_fts`; triggers `blocks_ai/ad/au`, `pages_ai/ad/au`; views `block_path_refs`, `page_properties`, `page_property_values`, `tasks`). Do **not** create `file_snapshots` (dropped by ADR-017; the merge base is kept in memory). Split triggers into `schema/v1_triggers.sql` so the cold build can drop/recreate them.

`src/schema.rs`: `const SCHEMA_VERSION: u32 = 1`, `fn create(conn)`, `fn apply_pragmas(conn)` (WAL, `synchronous=NORMAL`, `foreign_keys=ON`, `temp_store=MEMORY`, `mmap_size=268435456`, `cache_size=-65536`, `busy_timeout=5000`). `rusqlite` with `bundled` feature; assert `sqlite_version() >= 3.45` at startup.

## Acceptance Criteria
- Test: fresh DB lists every object above in `sqlite_master` (and no `file_snapshots`); `PRAGMA journal_mode` = `wal`.
- Test: inserting/deleting a block keeps `blocks_fts` and `blocks_fts_tri` in sync; `ON DELETE CASCADE` removes refs and properties.
- Test: `block_path_refs` view returns ancestor refs for a 3-level outline.

## Notes
BIT-SP-0003.R3. ADR-004, ADR-017.
