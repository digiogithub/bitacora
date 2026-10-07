---
id: BIT-US-0155
type: story
title: Graph data read API in bitacora-index
status: backlog
priority: high
parent: BIT-EP-0024
milestone: BIT-M-0009
author: mcp
labels: [v2, graph, bitacora-index]
estimate: 5
created: 2026-10-07T09:19:45Z
updated: 2026-10-07T09:19:45Z
---

## Description
As the graph view, I need nodes and edges computed from the index with Logseq's filters, for the whole graph and for one page's neighbourhood.

## Acceptance Criteria
- `read/graph.rs`: `GraphData { nodes{id,name,is_journal,is_tag,is_namespace_parent,degree}, edges }` from `block_page_refs` (kinds link/tag/property/embed), `page_tags`, `namespace_parent_id`; placeholder pages included; self-links, UUID-like and asset names excluded.
- Filters: journals, orphans, builtin, `exclude-from-graph-view::`.
- Local graph API (1-hop + neighbour links, journals optional).
- Fixture tests incl. rebuild-from-scratch equality; read-only.

## Notes
Implements BIT-SP-0012.R1, BIT-SP-0012.R2. Logseq behaviour: `handler/graph.cljs:84-175`, `db/model.cljs:1178-1200`.
