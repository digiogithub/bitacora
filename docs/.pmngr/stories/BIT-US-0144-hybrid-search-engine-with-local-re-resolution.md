---
id: BIT-US-0144
type: story
title: Hybrid search engine with local re-resolution
status: done
priority: high
parent: BIT-EP-0021
milestone: BIT-M-0007
author: mcp
labels: [v2, search, bitacora-pando, bitacora-index]
estimate: 5
created: 2026-10-07T09:16:57Z
updated: 2026-10-07T11:03:30Z
started: 2026-10-07T11:03:22Z
closed: 2026-10-07T11:03:30Z
---

## Description
As a user, I want search results that combine exact matches and meaning, always showing my current local content.

## Acceptance Criteria
- Semantic query via `KbClient::search` with `path_prefix = bitacora/<graph_id>/`, hits resolved by block uuid in the local index, stale hits dropped.
- RRF (k=60, existing `search/mod.rs`) merge of FTS5 and semantic lists; raw scores hidden.
- Timeout budget; on Pando failure returns lexical results plus an `semantic_unavailable` flag (test).

## Notes
Implements BIT-SP-0010.R4, BIT-SP-0009.R4. Reference git-in-track `vault/semantic.go`, `search_code.go#semanticMerge`.
