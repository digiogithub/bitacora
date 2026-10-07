---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - design
    - pando
    - search
---
# Semantic search: document model and sync

How the graph is mirrored into Pando's shared knowledge base for semantic search. Plan: [[bitacora-v2-plan]] (D4). Decision: ADR-030 in [[architecture]]. Integration basics (settings, consent, service): [[pando-integration]]. Code: `crates/bitacora-pando/src/semantic/`. Only Pando's current generic REST API is used (`POST` / `DELETE /api/v1/remembrances/kb/documents`); nothing here needs a Pando change.

## 1. Document model (BIT-US-0142)

One eligible block = one document.

| Part | Value |
| --- | --- |
| id (`file_path` in Pando) | `bitacora/<graph_id>/<block-uuid>` |
| text | `# <page title>`, then `> <ancestor> > <ancestor>` when the block is nested, a blank line, the block content without bookkeeping properties (`id::`, `collapsed::`, `created-at::`, ...) and without `:LOGBOOK:` drawers |
| metadata | `source`, `graph`, `graph_name`, `page`, `page_path`, `block_uuid`, `journal_day` (journals), `tags`, `marker`, `schema`, `content_hash` |
| tags | the page `tags::` followed by the block's `#tags` |
| `content_hash` | first 32 hex chars of blake3 over text and metadata (without the hash itself); includes `schema` so a layout change re-sends everything |

`bitacora_pando::semantic::map_block` is a pure function of a `bitacora_index::SemanticBlock` (page context read by `IndexReader::semantic_blocks` / `semantic_block`) and a `ContentPolicy`; the same input always gives the same hash. A page rename changes the text of that page's blocks only, so only those are re-sent; other pages are untouched.

### 1.1 Stable identifiers

* `graph_id` is `bitacora_index::graph_id(root)`: 16 hex chars of blake3 over the canonical absolute graph path. It is machine-local, deterministic (it survives index rebuilds and needs no state) and nothing is written into the graph. Moving the graph folder changes it; the old documents then stay in Pando until purged, which is acceptable for a machine-local integration.
* The block uuid is `id::` when the file has one, otherwise the index uuid. The index keeps generated uuids across incremental reparses (uuid carry-over, ADR-006), so editing a block never changes its document id. A **full index rebuild** may assign new uuids to blocks without `id::`: the ledger then holds ids that are no longer eligible, the diff deletes them and sends the new ones. This costs one re-send of those blocks and is the price of never writing `id::` into user files just for indexing (rule 1).

  Evaluated (BIT-US-0144): remapping old ids to new ones by content hash without re-embedding is **not possible** with Pando's generic API. It only offers upsert and delete keyed by `file_path`; there is no move or rename, and a new `file_path` is embedded from scratch. A ledger-side alias (keep sending under the old id while the block has a new uuid) would decouple the document id from the block uuid in the mapping, the hash, the worker and search re-resolution, and is ambiguous for blocks with identical text. Not worth it: only blocks without `id::` are affected, and only a *full* rebuild (index schema or parser version bump, corruption recovery) re-generates uuids, never an edit or a reconcile. The cost is one re-embedding of those blocks per rebuild; users who want zero churn can give blocks `id::` through Logseq's own block references.

### 1.2 Eligibility (BIT-SP-0010.R2)

`ContentPolicy` (built from `GraphConsent` by `from_consent`):

* exclusions: an entry with `/` is a graph-relative path prefix (`pages/work/`); any other entry is a page name, matching the page, its `ns/` children and any block tagged with it. Case-insensitive.
* `private:: true` (the `privacy_property`, default `private`) on the page (page-properties pre-block) or on the block itself.
* the page-properties pre-block, journals when `include_journals` is off, property-only blocks, asset-only blocks (images, files, `{{video}}`, `{{audio}}`, `{{pdf}}` ...), and blocks with fewer than `min_chars` (default 20) letters and digits once task markers and asset embeds are removed.

Changing the policy (`SemanticWorker::set_policy`) re-diffs the graph: documents that stopped being eligible are deleted, newly eligible ones are sent.

## 2. Sync (BIT-US-0143)

```
IndexWriter events --> diff thread --> outbox ---> sender (tokio) --> KbClient upsert / delete
                          ^              |                |
                          +-- DocSource  +-- ledger  <-----+  (state updated on every ack)
```

### 2.1 The ledger lives outside the index

The ledger records what exists on the server. The index database is a cache that is dropped and rebuilt on schema, parser or corruption events; if the ledger lived there, a rebuild would forget remote documents and leak orphans. It must not be in the graph folder either (rule 1, and it would travel with git). It is therefore a small SQLite file next to the index, `<data_dir>/bitacora/graphs/<graph-id>/semantic.sqlite` (`LEDGER_FILE`), never touched by index recovery. SQLite rather than JSON because an enqueue plus its bookkeeping must be atomic across crashes, and the project already ships it.

* `semantic_state(doc_id, block_uuid, path, content_hash, synced_at)`: what the server acknowledged. Source of truth for purge and orphan cleanup; no remote listing exists or is needed.
* `semantic_outbox(doc_id, op, block_uuid, path, hash, seq, attempts, next_at, last_error)`: one pending operation per document (the newest intent replaces the older); the payload is not stored, the sender re-reads the block from the index when it sends, so it always ships the freshest eligible content and turns an upsert of a block that became ineligible into a delete.
* `semantic_meta`: the remote the ledger describes (`managed:<graph_id>` in managed mode, the REST URL otherwise). Opening it for another remote, or with another `user_version`, empties it: those documents live elsewhere, so everything is sent again.

If the ledger file is lost, everything is re-sent (upserts are idempotent) but documents of blocks deleted meanwhile are orphaned on the server; a search with `path_prefix = bitacora/<graph_id>/` can find them (resync/purge story, BIT-SP-0010.R6).

### 2.2 Diffing

`Reconciler::sync_file(path)` compares the eligible documents of a file with what the ledger believes (state overlaid with pending outbox rows for that path): changed or new hash -> upsert; tracked id no longer in the file -> delete, unless the block now lives in another file (a move: the upsert for the new location replaces the document, never delete-then-create). Events: `FileReplaced`, `FileDeleted` and `FileRenamed` re-diff the touched paths (coalesced for a burst); `BulkFinished` and the start of the worker re-diff every file. An index that lists no files never causes deletions (it is rebuilding, not describing an empty graph).

Enqueue rules keep the outbox minimal: an upsert whose hash equals the acknowledged one cancels any pending operation; a delete of a document never sent just drops the pending upsert, unless that upsert was already attempted (it may have reached the server), in which case it becomes a delete.

### 2.3 Sender

An async task on the Pando service runtime. It wakes on new work (then waits `debounce`, 1.5 s) or when a retry is due, checks the gate (service `Connected`; consent is already enforced by the service) and sends due rows with at most `concurrency` (4) requests in flight, `batch` rows per round. Errors: unreachable / timeout / 503 / 401 end the round and postpone the whole outbox with exponential backoff (2 s .. 5 min) without hammering the server; other failures back off per document; 4xx client errors are parked at the maximum delay and never block other documents. `DELETE` of a missing document counts as success. Every acknowledged operation updates the ledger in the transaction that removes the outbox row (only if the row was not replaced meanwhile). Progress is published as `PandoEvent::SyncProgress { done, total }`.

### 2.4 Wiring

`bitacora_pando::semantic::start_session` (called by `Session::open` after the service starts) returns `None` unless the integration is active with consent for the graph and the `semantic_search` feature is on, so nothing is read or sent otherwise. `Session::semantic()` / `semantic_status()` expose the worker (`set_policy`, `reconcile`, `purge`, `retry_now`, counts); shutdown stops it before the Pando service and leaves pending rows in the ledger for the next session.

## 3. Hybrid search (BIT-US-0144)

Code: `semantic/search.rs` (`HybridSearch`), runtime API `Session::hybrid_search(query, &HybridOptions) -> HybridResults`. The UI (BIT-US-0145) and MCP/CLI (BIT-US-0146) call this one entry point.

1. **Lexical**: the local FTS5 pipeline (`IndexReader::search`, already fused internally) always runs.
2. **Semantic**: in parallel, `KbClient::search` with `path_prefix = bitacora/<graph_id>/` (so other apps' and graphs' documents never match), at most 20 results, under `HybridOptions::timeout` (2.5 s default).
3. **Local re-resolution**: Pando's answer is only a list of doc ids. Each id is parsed back to a block uuid and read from the index through the same `DocSource` the sync uses, with the *current* policy. A hit is dropped when its id is not ours, the block no longer exists, or the block is now excluded or ineligible (the server copy may lag behind a local edit or an exclusion change). The snippet and page title come from the local block, never from Pando's chunk text. A surviving hit whose `content_hash` differs from the local one is kept and flagged `stale`. Several chunks of one block count once.
4. **Fusion**: Reciprocal Rank Fusion with `k = 60` (same constant as the lexical pipeline) over the lexical list and the semantic list, keyed by block uuid (pages by id). A block in both lists outranks single-list hits; lexical highlights are kept.

### 3.1 Degradation

`HybridResults::semantic` tells the caller what happened: `Used { candidates, dropped }` or `Unavailable(Disabled | Offline | Timeout | Failed(msg))`. `Disabled`: no worker (Pando off, no consent for the graph, or the `semantic_search` feature is off). `Offline`: the service is not `Connected` or Pando is unreachable (no request is made when the gate is closed). `Timeout`, `Failed`: slow or erroring Pando. In every case the lexical hits are returned unchanged and nothing is logged with content. `search` blocks, so call it from a background thread, not from the UI thread.

Lexical results are *not* filtered by the exclusions: those only govern what leaves the machine, and the user's own local search keeps finding their own pages.

## Requirements

- MUST NOT write `id::` or any other marker to user files for indexing.
- MUST send an upsert only when the content hash differs from the acknowledged one.
- MUST keep the ledger outside the rebuildable index and the graph folder.
- MUST NOT contact Pando for a graph without consent.
- SHOULD use bounded concurrency and backoff; a Pando outage must not slow the editor.
- MUST scope semantic search to the graph prefix and re-resolve every hit against the local index; MUST fall back to lexical results when Pando is unavailable.

## Open questions

- Orphan cleanup after the ledger file is lost, and after moving the graph folder (new `graph_id`): owned by the resync/purge story (BIT-SP-0010.R6).
- Whether very large blocks should be split into several documents (currently one document per block; Pando chunks long texts itself).
