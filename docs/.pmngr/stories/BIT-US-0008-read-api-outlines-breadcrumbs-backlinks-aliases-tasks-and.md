---
id: BIT-US-0008
type: story
title: "Read API: outlines, breadcrumbs, backlinks, aliases, tasks and namespaces"
status: backlog
priority: high
parent: BIT-EP-0005
milestone: BIT-M-0002
author: mcp
labels: [index, bitacora-index]
estimate: 5
created: 2026-10-06T14:25:24Z
updated: 2026-10-06T14:25:24Z
---

## Description
As a UI or MCP developer, I want a typed read API over the index for page outlines, subtrees, ancestors, linked and unlinked references, block refs, alias closure, namespaces, tasks/agenda and graph edges, so that views and MCP tools never write SQL themselves.

## Acceptance Criteria
- Queries from [[sqlite-index-schema]] §5 are exposed as functions on an `IndexReader` returning domain structs (no `rusqlite` types leak).
- Page outline supports pagination (50 + 25) and collapsed-subtree skipping.
- Linked references use path-refs ∩ 2-hop alias closure minus own page, grouped by page, with `filters::` support.
- Unlinked references use FTS prefilter + Logseq regex; no full scan.
- Results on the fixture graph match the expected Logseq output recorded in test fixtures.

## Notes
Implements: BIT-SP-0003.R4, BIT-SP-0003.R9, BIT-SP-0003.R17. See [[sqlite-index-schema]] §5, [[03-parsing-indexing-search]] §4, §6.
