---
id: BIT-US-0018
type: story
title: Read tools for backlinks, tasks, queries, graph info and sync status
status: in_progress
priority: medium
parent: BIT-EP-0010
milestone: BIT-M-0002
author: mcp
labels: [mcp, tools, read, query]
estimate: 5
created: 2026-10-06T14:27:12Z
updated: 2026-10-06T18:44:25Z
started: 2026-10-06T18:44:25Z
---

## Description
As an AI agent, I want backlinks, task lists, simple queries and graph/sync status, so that I can do reviews and planning over the graph without scanning every page.

Tools: `backlinks`, `tasks`, `query` (Logseq simple-query DSL subset via the index's DSL→SQL compiler), `get_graph_info`, `list_graphs`, `git_sync_status`.

## Acceptance Criteria
- `backlinks {name|uuid, include_unlinked}` groups referencing blocks by page with breadcrumbs.
- `tasks` filters by status set, page, scheduled/deadline bounds, priority.
- `query` accepts e.g. `(and (task TODO) [[Project X]] (between -7d today))`; Datalog input → `INVALID_QUERY`.
- `git_sync_status` reports state, ahead/behind, last sync, conflict count/pages, last error (reads the sync engine's watch channel; returns `Disabled` when sync is off).
- All are `readOnlyHint` and covered by tool-level tests on a fixture graph.

## Notes
Implements: BIT-SP-0007.R11, BIT-SP-0007.R16. See [[mcp-server]] §5.1, [[sqlite-index-schema]] (query DSL), [[git-sync-merge]] §6.
