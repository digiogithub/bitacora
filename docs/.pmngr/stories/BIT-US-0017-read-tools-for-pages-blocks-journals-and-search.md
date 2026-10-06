---
id: BIT-US-0017
type: story
title: Read tools for pages, blocks, journals and search
status: backlog
priority: high
parent: BIT-EP-0010
milestone: BIT-M-0002
author: mcp
labels: [mcp, tools, read]
estimate: 8
created: 2026-10-06T14:27:12Z
updated: 2026-10-06T14:27:12Z
---

## Description
As an AI agent, I want to search the graph and read pages, block trees, single blocks and journals with Logseq-like tool names, so that I can answer questions about the user's notes.

Tools: `search`, `get_page`, `get_page_blocks_tree` (alias `get_block_tree`), `get_block`, `list_pages`, `list_journals`, `get_today_journal` (read-only: `create_if_missing` refused until writes are enabled).

## Acceptance Criteria
- Each tool has `schemars` input + output schemas, `readOnlyHint: true`, and a description naming the Logseq plugin API it mirrors.
- Page names resolve case-insensitively and through `alias::`; unknown page → `NOT_FOUND`.
- Every block/page carries `version` / `etag`.
- Results include `structuredContent` plus a Markdown text rendering; list tools paginate with `cursor`; text capped at ~20k chars with `truncated: true`.
- Optional `graph` argument selects a graph; default is active graph.

## Notes
Implements: BIT-SP-0007.R11, BIT-SP-0007.R16, BIT-SP-0007.R17. See [[mcp-server]] §5.1, §5.4, [[sqlite-index-schema]], [[05-git-and-apis]] §2.1.
