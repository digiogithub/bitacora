---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - pando
    - index
    - search
---
# Semantic document model and sync worker

Implements BIT-US-0142 (BIT-T-0438, BIT-T-0439, BIT-T-0440) and BIT-US-0143 (BIT-T-0441..0444) of [[bitacora-v2-plan]] (D4, ADR-030). Design: [[semantic-search]]; builds on [[pando-integration]].

## What changed
- `bitacora-index`: new read API `IndexReader::semantic_file_paths`, `semantic_blocks(path)`, `semantic_block(uuid)` returning `SemanticBlock` (page title, journal day, breadcrumb, content, marker, tags, block and page properties) in `read/semantic.rs`.
- `bitacora-pando` new module `semantic/`:
  - `doc.rs`: `map_block`, `ContentPolicy`, `GraphInfo`, `SemanticDoc`, `Skip`, `doc_id`; id `bitacora/<graph_id>/<block-uuid>`, text = page title + breadcrumb + content, metadata incl. `content_hash` and `schema`; filters (pre-block, `private::`, exclusions by path prefix / page name / tag, journals switch, property-only, asset-only, minimum 20 characters).
  - `ledger.rs`: machine-local `Ledger` (SQLite `semantic.sqlite` beside the index; `semantic_state`, `semantic_outbox`, `semantic_meta`), enqueue rules, `complete`/`fail`/`defer_all`/`purge_all`, bound to the remote.
  - `source.rs`: `DocSource` trait and `IndexSource` over `IndexReader`.
  - `worker.rs`: `Reconciler` (hash diff per file, cold-start reconcile, move-safe deletes), `SemanticWorker` (diff thread on index events + tokio sender with debounce, bounded concurrency, global offline backoff, per-document backoff, progress events), `attach`.
  - `session.rs`: `start_session` used by the runtime (feature and consent gated).
- `service.rs`: `ServiceProbe` + `PandoService::probe()` (client and connected state for background workers).
- `bitacora-runtime` `live.rs`: `Session` starts the worker after the Pando service, exposes `semantic()` / `semantic_status()`, stops it before the service.
- Docs: ADR-030 row in `docs/architecture.md`, `docs/design/semantic-search.md`.

## Decisions
- Ledger is a separate SQLite file in the per-graph data dir, not the index DB (rebuilt on schema/parser events) and not the graph folder (rule 1).
- `graph_id` is the existing path-derived `bitacora_index::graph_id` (no state, nothing written to the graph).
- The outbox stores no payload; the sender re-reads the block from the index when sending.
- Generated block uuids change on a full index rebuild; the ledger turns that into delete + upsert.

## Verification
- `cargo clippy -p bitacora-pando -p bitacora-index -p bitacora-runtime -p bitacora-cli --all-targets --locked -- -D warnings` clean; `cargo xtask check-deps` OK.
- Tests: `bitacora-pando` unit (21) + `semantic_sync` (14, axum mock KB: single edit, burst collapse, offline then restart then flush, bounded concurrency, moves, exclusions, purge, rejected document, progress, remote change, rebuild) + `semantic_index` (4: golden snapshot, fixture graphs, real index events end to end); `bitacora-index` `read_semantic`; `bitacora-runtime` `pando_semantic` (consent gate, restart sends nothing).
