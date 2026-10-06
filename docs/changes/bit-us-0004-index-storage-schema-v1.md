---
created_at: 2026-10-06T17:14:32.043856008Z
updated_at: 2026-10-06T17:14:32.043856008Z
tags:
    - change
    - index
---
# BIT-US-0004 Index storage, schema v1, open/validate

Part of [[bitacora-full-development-plan]]; design: [[sqlite-index-schema]] (§1.1, §3, §4.1 step 1), ADR-004/005.

## What changed
- New in `crates/bitacora-index`: `location.rs` (`graph_id`, `IndexLocation`), `schema.rs` + `schema_v1.sql` (DDL from design §3; `file_snapshots` omitted per ADR-017; `user_version` stamped by code), `index.rs` (`Index::open`, `OpenOptions`, `OpenOutcome`, `RebuildKind`, `RecreateReason`, `WriteConnection`), `pool.rs` (`ReaderPool`, `PooledReader`), `error.rs`.
- Workspace dep `blake3 = "1.8"` added (CC0/Apache, `cargo deny` ok); index crate now uses rusqlite, directories, parking_lot.

## Behaviour
- Path `<data_dir>/bitacora/graphs/<blake3(canonical graph path)[..16]>/index.sqlite`; refuses locations inside the graph.
- Open: missing/empty file -> create; `user_version` mismatch -> delete db+wal+shm and recreate; corrupt/not-a-db/failed `quick_check`/unreadable meta -> same. `parser_version` or `config_hash` differ -> `RebuildKind::FullReparse`; `normalizer_version` differs -> `FtsOnly`. Flags are not persisted until `WriteConnection::record_versions()` (survives crash mid-rebuild).
- Single write connection via `Index::take_writer()` (returned on drop); read-only pool (lazy, bounded, `query_only`).

## Verification
`cargo test -p bitacora-index --locked`: 18 integration + 1 unit pass; `cargo clippy --workspace --all-targets --locked -D warnings`, `cargo deny check`, `cargo xtask check-deps`, `cargo machete` clean.
