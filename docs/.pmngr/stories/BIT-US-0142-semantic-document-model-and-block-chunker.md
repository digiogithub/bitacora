---
id: BIT-US-0142
type: story
title: Semantic document model and block chunker
status: done
priority: high
parent: BIT-EP-0021
milestone: BIT-M-0007
author: mcp
labels: [v2, search, bitacora-pando, bitacora-index]
estimate: 5
created: 2026-10-07T09:16:57Z
updated: 2026-10-07T10:51:10Z
started: 2026-10-07T10:47:32Z
closed: 2026-10-07T10:51:10Z
---

## Description
As the semantic indexer, I need a deterministic mapping from indexed blocks to Pando documents, so that ids are stable and only eligible content is sent.

## Acceptance Criteria
- Stable `graph_id` stored in the index `meta` table (survives rebuilds via a graph-local id or deterministic derivation; decision documented).
- Block → document: id `bitacora/<graph_id>/<block-uuid>`, text = page title + breadcrumb + block content, metadata (graph, page, page_path, block_uuid, journal_day, tags, marker, content_hash, schema).
- `ContentPolicy` + min length + property-only/asset-only filters applied; golden tests over fixture graphs; ADR-030 + `docs/design/semantic-search.md`.

## Notes
Implements BIT-SP-0010.R1, BIT-SP-0010.R2. Blocks without `id::` use the index uuid; decide whether a stable uuid needs persisting (never write `id::` to user files just for indexing — rule 1).
