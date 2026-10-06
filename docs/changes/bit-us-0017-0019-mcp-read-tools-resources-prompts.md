---
created_at: 2026-10-06T19:01:04.116278773Z
updated_at: 2026-10-06T19:01:04.116278773Z
tags:
    - change
    - mcp
---
# MCP read tools, resources and prompts (BIT-US-0017, BIT-US-0018, BIT-US-0019)

Plan: [[bitacora-full-development-plan]]. Design: [[mcp-server]] (ADR-010). Continues [[changes/bit-us-0015-0016-mcp-server-skeleton-auth.md]]; data comes from [[changes/bit-us-0008-0010-read-api-and-cli-doctor.md]] and [[changes/bit-us-0009-full-text-search.md]].

## What changed (commit 29334d9)
- `bitacora-mcp`: `GraphReader` trait grown into the full read surface; `IndexGraphReader` (index_reader.rs) implements it over `IndexReader` + `search::search` + graph folder (page text, config, assets). Decision: adapter lives in mcp because `mcp -> index` is an allowed edge in xtask/src/deps.rs; no ADR change.
- Tools (tools.rs, handler.rs): search, get_page, list_pages, get_page_blocks_tree, get_block_tree, get_block, list_journals, get_today_journal, backlinks, tasks, query (subset), get_graph_info, list_graphs, git_sync_status. All with input/output schemas, readOnlyHint, structured JSON + Markdown text (20k cap), cursors, error codes. `render.rs` holds shared plumbing, `dates.rs` date helpers.
- `query.rs`: and/(task)/(priority)/[[page]]/"text" only; datalog INVALID_QUERY, others NOT_SUPPORTED (DSL compiler is BIT-US-0101).
- `status.rs`: `SyncStatusProvider`, `DisabledSync`; `McpServer::start_with_sync`.
- `resources.rs`: list/templates/read, subscriptions (legacy `resources/subscribe`, stateful) driven by IndexEvent forwarding; assets confined and capped.
- `prompts.rs`: daily_review, weekly_review, summarize_page, capture, logseq_syntax.
- Scope enforcement: read scope from TokenInfo in request extensions on every tool/resource/prompt call.
- `bitacora-cli serve` (cmd/serve.rs): opens index, reconciles, polls reconcile every 2 s, serves IndexGraphReader; new `--data-dir`.
- Deps: blake3, jiff (mcp); dev-deps bitacora-config, bitacora-testkit.
- Design doc: implementation notes in docs/design/mcp-server.md.

## Verification
fmt, clippy --workspace --all-targets --locked -D warnings, xtask check-deps, cargo deny, machete, typos clean. `cargo test -p bitacora-mcp`: 18 unit + 10 http + 9 read_tools (real axum server, real index of logseq-docs copy, SSE notification test); `cargo test -p bitacora-cli`: 2 unit (incl. serve end-to-end over HTTP: initialize, tools/list, get_page, tree, search, graph info) + 5 index commands.

## Follow-ups
Real sync provider from app/cli; watcher instead of 2 s polling; write tools (BIT-US-0020+); DSL compiler for `query`.
