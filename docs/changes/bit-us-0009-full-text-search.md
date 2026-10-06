---
created_at: 2026-10-06T18:35:38.556113075Z
updated_at: 2026-10-06T18:35:38.556113075Z
tags:
    - change
    - index
    - search
---
# BIT-US-0009 Full-text search (T-0068..0073)

Continues [[bitacora-full-development-plan]]; design [[sqlite-index-schema]] §6 (new §6.3 notes), [[03-parsing-indexing-search]].

## What changed
- New module `crates/bitacora-index/src/search/`: `query.rs` (ParsedQuery -> safe FTS5 MATCH, trigram expr, LIKE patterns), `mod.rs` (`search`, `SearchOptions`, `Scope`, `SearchHit`, RRF k=60, `set_substring`, `substring_enabled`), `fuzzy.rs` (nucleo-matcher over `pages.search_title`), `snippet.rs` (raw-content windows + highlight ranges via folded->original offset map).
- `IndexWriter::set_substring(bool)` (writer.rs `Job::SetSubstring`); `fts_triggers_no_tri.sql` + `FTS_TRIGGERS_NO_TRI_SQL`; cold-build `finish_bulk`/`rebuild_fts` consult `sqlite_master` so a dropped trigram index survives bulk builds.
- Dependency: `nucleo-matcher` (workspace pin) in bitacora-index. `lib.rs` only gained `pub mod search;`.
- Design doc §6.3 added.

## Verification
- `cargo clippy --workspace --all-targets --locked -- -D warnings` clean; `cargo test -p bitacora-index --locked` all green (27 unit incl. 17 search unit, 10 integration in tests/search.rs); `cargo deny check`, `cargo machete`, `cargo xtask check-deps` OK.
- Benchmark `tests/bench_search.rs` (ignored, release): 5,200 pages / 51,500 blocks, 14 queries x 20 runs: p50 3.6 ms, p95 7.3 ms, max 8.0 ms (target p95 < 50 ms).

## Notes
- Substring stage runs only when the bm25 word search returns fewer than `limit` blocks.
- `search.substring` is not persisted/config-wired yet: callers apply it after `Index::open`. `dump::canonical_dump` still reads `blocks_fts_tri_docsize` (fails if the trigram index is dropped).
