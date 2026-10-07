---
id: BIT-SP-0010
type: spec
title: Semantic indexing and hybrid search
status: backlog
author: mcp
labels: [search, pando, index, v2]
created: 2026-10-07T09:08:04Z
updated: 2026-10-07T11:20:55Z
requirements:
  R1:
    status: backlog
    trace:
      code:
        - crates/bitacora-pando/src/semantic/doc.rs
        - crates/bitacora-index/src/read/semantic.rs
      tests:
        - crates/bitacora-pando/tests/semantic_index.rs
        - crates/bitacora-pando/src/semantic/doc.rs
  R2:
    status: backlog
    trace:
      code:
        - crates/bitacora-pando/src/semantic/doc.rs
        - crates/bitacora-pando/src/semantic/worker.rs
      tests:
        - crates/bitacora-pando/src/semantic/doc.rs
        - crates/bitacora-pando/tests/semantic_sync.rs
  R3:
    status: backlog
    trace:
      code:
        - crates/bitacora-pando/src/semantic/ledger.rs
        - crates/bitacora-pando/src/semantic/worker.rs
      tests:
        - crates/bitacora-pando/tests/semantic_sync.rs
        - crates/bitacora-runtime/tests/pando_semantic.rs
  R4:
    status: backlog
  R5:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/semantic.rs
        - crates/bitacora-runtime/src/mcp_semantic.rs
      tests: [crates/bitacora-mcp/tests/semantic_tools.rs]
  R6:
    status: backlog
    trace:
      code: [crates/bitacora-cli/src/cmd/semantic.rs]
      tests:
        - crates/bitacora-cli/src/cmd/semantic.rs
        - crates/bitacora-cli/src/main.rs
---

## Purpose
Define semantic indexing of graph blocks into Pando and hybrid (lexical + semantic) search in Bitacora.

## Scope
Document model, change detection, sync worker and outbox, search merge, UI and MCP exposure. Plan: [[bitacora-v2-plan]] (D4).

## Requirements

### BIT-SP-0010.R1 — Block-level semantic documents keyed by block uuid

The system SHALL index each eligible block as one document in Pando's shared KB, with id `bitacora/<graph_id>/<block-uuid>`, using only Pando's generic REST KB API. `graph_id` is a stable machine-local id for the graph. The document text SHALL be the page title, the parent-block breadcrumb and the block content. The metadata SHALL include graph, page, page path, block uuid, journal day (if any), tags, marker and content hash. A page rename SHALL NOT re-send blocks whose embedded text did not change: their document id is uuid-based and the page title is updated by re-sending only those affected.

#### Scenario: Document identity
- GIVEN block `6522…` on page "Project X" in graph G
- WHEN it is indexed
- THEN Pando holds a document `bitacora/G/6522…` whose text starts with "Project X" and the breadcrumb

#### Scenario: Unchanged block not re-sent
- GIVEN a synced block whose content hash and breadcrumb are unchanged
- WHEN the graph is reindexed locally or the app restarts
- THEN no upsert for that block is sent

### BIT-SP-0010.R2 — Exclusion rules for semantic indexing

The system SHALL NOT send to Pando: blocks on pages with `private:: true` (or a configured privacy property), blocks under configured excluded pages, namespaces or tags, property-only blocks, asset-only blocks and blocks shorter than a configurable minimum length (default 20 characters). Changing exclusions SHALL delete newly excluded documents from Pando.

#### Scenario: Private page
- GIVEN page "Health" has `private:: true`
- WHEN the graph is indexed semantically
- THEN no document with a block from "Health" exists in Pando

#### Scenario: New exclusion
- GIVEN namespace "work" was indexed
- WHEN the user adds "work" to excluded namespaces
- THEN delete requests are queued for every document of pages in "work"

### BIT-SP-0010.R3 — Incremental sync by content hash with durable outbox

The system SHALL drive semantic sync from index change events, SHALL send an upsert only for blocks whose `content_hash` differs from the last synced hash, SHALL delete the Pando documents of removed blocks, and SHALL persist pending operations in a SQLite outbox so they survive restarts and Pando outages. All semantic state SHALL be rebuildable from the graph (SQLite is a cache).

#### Scenario: Single edit
- GIVEN a synced graph
- WHEN the user edits one block
- THEN exactly one upsert for that block is sent after the debounce

#### Scenario: Offline period
- GIVEN Pando is stopped and the user edits 5 blocks and deletes 1
- WHEN Pando comes back
- THEN 5 upserts and 1 delete are sent and the outbox is empty

#### Scenario: Cold start
- GIVEN a synced graph and an app restart with no changes
- THEN no upsert is sent

### BIT-SP-0010.R4 — Hybrid ranking with local re-resolution of semantic hits

Search SHALL merge FTS5 and semantic results with reciprocal rank fusion (k = 60). Every semantic hit SHALL be re-resolved against the local index by block uuid; hits that no longer resolve SHALL be dropped and displayed content SHALL always come from the local index, never from Pando's stored text. Raw Pando scores SHALL NOT be shown.

#### Scenario: Stale hit
- GIVEN Pando still returns block X which was deleted locally a moment ago
- WHEN the user searches
- THEN X does not appear in the results

#### Scenario: Semantic-only match
- GIVEN a block "kept forgetting to water the ficus" and query "plants care"
- THEN the block appears in hybrid results even though FTS5 has no match

### BIT-SP-0010.R5 — Semantic search in palette, related blocks and MCP

The search palette SHALL offer a hybrid mode (lexical + semantic) when semantic indexing is available for the graph. The right panel SHALL offer "Related blocks" for the current block or page. `bitacora-mcp` SHALL expose a read-only `semantic_search` tool (and related-blocks), authenticated, audited and refusing with a clear error when semantic indexing is disabled for the graph.

#### Scenario: MCP tool disabled
- GIVEN semantic indexing is disabled for graph G
- WHEN an MCP client calls `semantic_search`
- THEN it receives a tool error "semantic search is not enabled for this graph" and the call is audited

### BIT-SP-0010.R6 — Semantic index can be resynced and purged per graph

The user SHALL be able to see per-graph semantic index status (documents synced, pending, last error), trigger a full resync, and purge all documents of a graph from Pando, from both the settings panel and `bitacora-cli semantic {status,resync,purge}`. Purging SHALL remove every document under the graph's `bitacora/<graph_id>/` prefix.

#### Scenario: Purge
- WHEN the user purges graph G
- THEN a subsequent Pando KB search with `path_prefix = bitacora/G/` returns no results
