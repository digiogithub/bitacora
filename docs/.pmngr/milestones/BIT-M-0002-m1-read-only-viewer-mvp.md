---
id: BIT-M-0002
type: milestone
title: M1 — Read-only viewer (MVP-α)
status: done
author: mcp
created: 2026-10-06T14:20:19Z
updated: 2026-10-07T08:21:50Z
started: 2026-10-07T00:15:15Z
closed: 2026-10-07T08:21:50Z
due: 2027-01-31
---

## Description
Open an existing Logseq graph, parse it losslessly, build the SQLite index, and browse it: journals, pages, page refs, backlinks, search. MCP server exposes read-only tools over Streamable HTTP.

## Acceptance Criteria
- Opening a 5,000-page graph indexes in < 30 s cold and < 2 s warm.
- Journals view, page view with rendered blocks, linked references, full-text search work.
- MCP read tools (search, get_page, get_block_tree, backlinks, list_journals) usable from an MCP client.
- No file in the graph is modified.

## Notes
See [[architecture]] §6.
