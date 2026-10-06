---
id: BIT-T-0300
type: task
title: DSL to SQL compiler with result-type rule and NULL-safe negation
status: done
priority: high
parent: BIT-US-0101
milestone: BIT-M-0005
author: mcp
labels: [bitacora-index, query]
estimate: 5
created: 2026-10-06T14:33:07Z
updated: 2026-10-06T19:05:49Z
started: 2026-10-06T18:49:11Z
closed: 2026-10-06T19:05:49Z
---

## Description
`crates/bitacora-index/src/query/dsl/compile.rs`: `compile(q: &Q, ctx: &QueryCtx) -> CompiledQuery { sql, params, result: Blocks|Pages, order, limit }`.
- Result type: blocks if any `PageRef` (outside page-level leaves), `Text`, `Between`, `Property`, `Task`, `Priority`, `Page` appears; else pages (`query_dsl.cljs:393-397`).
- Resolve names → page ids first; unknown → empty set predicate.
- Leaf mapping exactly per [[sqlite-index-schema]] §7.2 table (path-refs interval subquery for `[[x]]`, `blocks_fts_tri` for text ≥ 3 chars else `instr`, `block_property_values` for properties, `page_property_values`, journal-day `between`, timestamp `between` with fallback to `created_at`/`updated_at` columns, etc.). In block mode wrap page-level leaves as `b.page_id IN (SELECT p.id FROM pages p WHERE …)`.
- Every leaf emitted as `id IN (…)` or wrapped with `COALESCE(…, 0)` so `NOT` keeps non-tasks.
- `SortBy` → `ORDER BY` per table; `Sample` → `ORDER BY random() LIMIT n`; exclude `ctx.current_block`.

## Acceptance Criteria
- Snapshot tests of generated SQL for the 4 examples of §7.3.
- Execution tests matching BIT-SP-0003.R18 scenarios.
- All SQL uses bound parameters (no string interpolation of user values).

## Notes
BIT-SP-0003.R18.
