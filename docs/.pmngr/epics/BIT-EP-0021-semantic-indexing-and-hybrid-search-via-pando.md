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
updated: 2026-10-07T09:10:54Z
---

## Description
Index graph blocks into Pando's KB as block-level documents (`bitacora/<graph_id>/<block-uuid>`), driven by `IndexWriter::subscribe` events with a durable SQLite outbox and content-hash diffing; hybrid search (FTS5 + semantic, RRF k=60) with local re-resolution; palette hybrid mode, related-blocks panel, MCP `semantic_search` tool, CLI `semantic {status,resync,purge}`.

## Acceptance Criteria
- BIT-SP-0010.R1-R6 satisfied and verified.
- Works against current Pando with client-side workarounds; uses batch/list/hash-skip endpoints when the Pando version supports them.

## Notes
- Plan [[bitacora-v2-plan]] decision D4 (ADR-030). git-in-track reference: `internal/pando/`, `vault/semantic.go`, `search_code.go#semanticMerge`.
- Index schema changes need a migration and version bump (rule 5).
