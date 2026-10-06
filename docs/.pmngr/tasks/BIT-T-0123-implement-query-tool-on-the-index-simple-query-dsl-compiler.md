---
id: BIT-T-0123
type: task
title: Implement query tool on the index simple-query DSL compiler
status: backlog
priority: medium
parent: BIT-US-0018
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, tools, query]
estimate: 2
created: 2026-10-06T14:29:55Z
updated: 2026-10-06T14:29:55Z
---

## Description
`crates/bitacora-mcp/src/tools/query.rs`: `query {dsl, limit?}` calling `bitacora_index::query::compile_and_run(dsl)`; Datalog-looking input (`[:find` / `{:query`) or parse errors → `INVALID_QUERY` with the parser position and the list of supported operators (`and/or/not`, `[[page]]`, `#tag`, `task`, `priority`, `page-property`, `property`, `between`, `page`, `full-text-search`, `sort-by`). Description mirrors `DB.q`.

## Acceptance Criteria
- Tests for 6 representative DSL queries plus Datalog rejection.

## Notes
Story BIT-US-0018. Implements BIT-SP-0007.R11. Depends on index query DSL ([[sqlite-index-schema]]).
