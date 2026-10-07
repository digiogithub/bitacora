---
created_at: 2026-10-07T15:00:00Z
updated_at: 2026-10-07T15:00:00Z
tags:
    - change
    - pando
    - search
---
# Hybrid search engine with local re-resolution (BIT-US-0144)

Continues [[bitacora-v2-plan]] (D4) and [[semantic-search]] (ADR-030, BIT-US-0142/0143).

## What changed

- `crates/bitacora-pando/src/semantic/search.rs` (new): `HybridSearch`, `HybridOptions`, `HybridResults`, `HybridHit`, `HybridTarget`, `SemanticState`, `Unavailable`, `Remote`. FTS5 (`IndexReader::search`) and Pando `KbClient::search` (scoped by `path_prefix = bitacora/<graph_id>/`, timeout 2.5 s) run in parallel; Pando hits are re-resolved through `DocSource::block_doc` with the current `ContentPolicy` (unknown, stale-deleted, excluded or foreign hits dropped; snippets come from local content), then fused with RRF (k = 60). Any Pando problem degrades to lexical-only and is reported in `HybridResults::semantic`.
- `SemanticWorker::policy()` exposes the shared policy so exclusion edits apply to search immediately.
- `bitacora-runtime`: `Session::hybrid_search(query, &HybridOptions)`, `RuntimeError::Search`, re-exports of the result types. Always available (lexical-only without Pando).
- `docs/design/semantic-search.md`: section 3 (hybrid search), the rebuild-uuid-churn evaluation, requirements.

## Rebuild uuid churn (evaluated, not changed)

Remapping ids by content hash without re-embedding is impossible with Pando's generic API (upsert/delete by `file_path`, no move). A ledger alias would decouple doc id from block uuid across mapping, hash, worker and search, and is ambiguous for duplicate text. Cost documented instead: only blocks without `id::`, only on a full index rebuild.

## Verification

- `cargo test -p bitacora-pando` (unit fusion tests, `tests/semantic_search.rs` with a mock Pando over a real index: fusion, graph prefix, re-resolution drops, 503, timeout, closed gate, no remote, blank query).
- `cargo test -p bitacora-runtime --test pando_semantic` (lexical-only without Pando and without consent).
- `cargo clippy -p bitacora-pando -p bitacora-runtime --all-targets --locked -- -D warnings`.
