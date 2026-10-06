---
created_at: 2026-10-06T17:54:15.235801291Z
updated_at: 2026-10-06T17:54:15.235801291Z
tags:
    - change
    - index
---
# BIT-US-0006 / BIT-US-0007: index writer, UUID carry-over, reconcile and cold build

Part of [[bitacora-full-development-plan]]; design [[sqlite-index-schema]] (section 4.7 added), builds on [[bit-us-0004-index-storage-schema-v1]] and [[bit-us-0005-parsedfile-index-rows]]. Commit eac3a54. ADR-004/005/006/017 (T-0040 cancelled).

## What changed (crates/bitacora-index)
- `replace.rs`: `replace_file` (design 4.4 steps 0-10), `delete_file`, `rename_file_row`, `touch_file`, `seed_builtin_pages` (16 built-ins), placeholder GC to fixpoint, `FileInput`, `FileKind`, `page_uuid` (UUIDv5). Identical blocks (same depth+content hash, non-pre-block) keep their rows; others deleted/inserted.
- `carry.rs`: `assign_uuids` (id:: > Myers diff carry-over via `similar`, similarity >= 0.5 > UUIDv7; cross-file duplicate id gets fresh UUID + diagnostic).
- `writer.rs`: `IndexWriter` thread, bounded job channel, `Pending<T>`, `IndexEvent`, bulk mode (`begin_bulk`/`end_bulk`: triggers dropped, FK off, 200 files/tx, FTS rebuild, `foreign_key_check`, ANALYZE, `meta.bulk_in_progress` crash flag), `renormalize_search` for `RebuildKind::FtsOnly`.
- `reconcile.rs`: `Indexer` (start/reconcile/handle/update_path/refresh_duplicates/request_priority), `FsChange {Modified, Deleted, Renamed, Overflow}`, `IndexerOptions`, `ReconcileStats`; rayon parse waves in priority order (today/home, requested, journals newest first, pages).
- `config_hash.rs` (`config_hash`, `OpenOptions::for_config`), `dump.rs` (`canonical_dump`), `error.rs` new variants, `schema.rs`/`fts_triggers.sql` (triggers split out so they can be recreated).
- Workspace deps: `uuid` gains `v5`, new `rayon = "1.12"`; index uses `similar`, dev `bitacora-testkit`, `proptest`.

## Decisions
- Duplicate page titles: smallest path owns the page (order independent); others `duplicate_page`; `refresh_duplicates` handles handover. Explicit `id::` clashes stay first-writer-wins.
- Watcher events always hash (no mtime trust); touch moves a mtime-derived `pages.updated_at`.

## Verification
fmt, `cargo clippy --workspace --all-targets --locked -D warnings` clean; `cargo test -p bitacora-index --locked`: 10 unit + 18 lifecycle + 3 golden + 16 parse + 2 property (40+12 cases, run 6 times) + 8 reconcile + 11 writer pass (+2 ignored benches); check-deps, cargo deny, machete OK. Release benches: 500-block page replace median 2.8 ms; 50 080-block cold build 0.83 s (37 MiB), warm reconcile 2.7 ms.

## Handoffs
bitacora-watch must emit `FsChange`; core's writer should call `IndexWriter`/`Indexer` after own writes; gintrack `verify_requirement` needs ingested test results (not done).
