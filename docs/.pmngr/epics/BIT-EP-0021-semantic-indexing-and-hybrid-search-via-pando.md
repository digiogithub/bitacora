---
id: BIT-EP-0021
type: epic
title: Semantic indexing and hybrid search via Pando
status: backlog
priority: high
milestone: BIT-M-0007
author: mcp
labels: [v2, pando, search, bitacora-index, bitacora-pando]
created: 2026-10-07T09:10:54Z
updated: 2026-10-07T09:54:52Z
---

## Description
Index graph blocks into Pando's shared KB as block-level documents (`bitacora/<graph_id>/<block-uuid>`) using **only Pando's current generic REST API** (per-document upsert/delete, search with `path_prefix`):
- sync is driven by `IndexWriter::subscribe` events, with a machine-local sync ledger and outbox and content-hash diffing;
- hybrid search (FTS5 + semantic, RRF k=60) with local re-resolution;
- palette hybrid mode, related-blocks panel, MCP `semantic_search` tool, CLI `semantic {status,resync,purge}`.

## Acceptance Criteria
- BIT-SP-0010.R1-R6 satisfied and verified.
- No Pando server changes required.

## Notes
- Plan [[bitacora-v2-plan]] decision D4 (ADR-030). Owner decision 2026-10-07: client-side solutions; Pando only gets generic features.
- Client-side replacements:
  - the ledger records what was sent, so no remote listing is needed;
  - only changed hashes are sent, so no server hash skip;
  - bounded concurrent per-document calls, so no batch endpoint;
  - purge deletes the ids from the ledger, so no delete-by-prefix.
- REST upsert writes no mirror file, and KB directory sync only deletes documents with `source_path` metadata (verified in Pando `handlers_remembrances_kb.go`, `rag/kb/sync.go`).
- git-in-track reference: `internal/pando/`, `vault/semantic.go`, `search_code.go#semanticMerge`.
