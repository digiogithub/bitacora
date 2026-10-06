---
created_at: 2026-10-06T19:12:09.82818837Z
updated_at: 2026-10-06T19:12:09.82818837Z
tags:
    - change
    - mcp
    - query
---
# BIT-T-0123 MCP `query` tool on the DSL compiler and advanced engine

Follow-up of [[changes/bit-us-0017-0019-mcp-read-tools-resources-prompts.md]] and [[changes/bit-us-0101-0103-query-dsl-and-datalog.md]]. Story: BIT-US-0018. See [[mcp-server]] and [[sqlite-index-schema]].

## What changed
- New `GraphReader::run_query(&QueryRequest) -> QueryOutcome` (crates/bitacora-mcp/src/reader.rs); default returns NotSupported so `StaticGraphReader` keeps the old minimal subset in `query.rs` as fallback.
- `IndexGraphReader::run_query` (index_reader.rs): builds `QueryContext` (local today, `now_ms`, local-midnight `today_start_ms`, `current_page` resolved to the page key, `current_block`), then routes `#+BEGIN_QUERY`, `{...}` EDN maps and bare `[:find` vectors to `IndexReader::query_advanced`, everything else to `query_simple`.
- Errors: `QueryError::Syntax` -> new `ReaderErrorKind::InvalidQuery` -> `INVALID_QUERY`; `QueryError::Unsupported(c)` -> `NOT_SUPPORTED` with message `unsupported: <c>`. Ignored constructs (`AdvancedOutcome.warnings`) are returned in `warnings`.
- `query` tool (tools.rs): new args `current_page`, `current_block`, `limit` (default 50, max 500), `cursor`; result `{kind: blocks|pages|rows, title, blocks, pages, columns, rows, total, warnings, next_cursor, truncated}`. Advanced results that are not purely block or page entities (aggregates, scalars) come back as `rows` of JSON cells.

## Behaviour notes
- Page references now follow Logseq semantics through the compiler (a task on page `Project X` matches `(and (task TODO) [[Project X]])`).
- Raw datalog is no longer rejected: the README scenario "Raw Datalog not supported" of BIT-SP-0007.R11 and the BIT-US-0018 criterion are superseded.

## Verification
- `cargo test -p bitacora-mcp --locked`: unit 18 + http 10 + read_tools 10 pass (new `query_tool_routes_simple_and_advanced_queries`: DSL, pagination, advanced map, bare vector, aggregate rows, current_page input, `:result-transform` warning, unsupported attribute error, syntax error). `cargo clippy -p bitacora-mcp --all-targets --locked -D warnings` clean.
- Note: `page_changes_notify_subscribed_resources_within_two_seconds` failed once under parallel load, then passed 3 consecutive runs (timing-sensitive, unrelated).
