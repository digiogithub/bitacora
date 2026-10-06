---
created_at: 2026-10-06T19:06:06.263646111Z
updated_at: 2026-10-06T19:06:06.263646111Z
tags:
    - change
    - index
    - query
---
# BIT-US-0101 / BIT-US-0103: query DSL and Datalog subset

Continues [[bitacora-full-development-plan]]; designs: [[sqlite-index-schema]] §7, §7.5, §8, §8.1; analysis [[03-parsing-indexing-search]] §8. Builds on [[bit-us-0008-0010-read-api-and-cli-doctor]] and [[bit-us-0009-full-text-search]].

## What changed
New module `crates/bitacora-index/src/query/` (lib.rs: `pub mod query;` only):
- `edn.rs`: EDN reader (`Edn`, `read_all`).
- `dsl.rs`: `pre_transform` (`[[x]]`, `#x`, `#[[x]]`), AST `Query`/`SimpleQuery`, `parse`, result-type rule `Query::returns_blocks`.
- `dates.rs`: `journal_day`, `timestamp` (today/yesterday/tomorrow, +-N d/w/m/y, now, h, min, `[[Journal title]]`).
- `compile.rs`: parameterised DSL to SQL (`compile`, `leaf_sql`); NULL-safe `not`; sort-by; sample; own-block exclusion.
- `mod.rs`: `QueryContext`, `QueryError` (`Syntax`, `Unsupported`, `Index`), `compile_simple`, `IndexReader::query_simple`, `IndexReader::query_advanced`.
- `advanced.rs`: `#+BEGIN_QUERY` reader, `AdvancedOutcome`, `Cell`, `Unsupported`, hydration, `bitacora_regexp` UDF.
- `datalog.rs`: typed-variable conjunctive compiler (patterns, DSL rules, predicates, functions, not/not-join/or/or-join, inputs, find specs/aggregates).
- Tests: `tests/query_simple.rs` (17 tests, ~120 assertions), `tests/query_advanced.rs` (3 tests), corpus `fixtures/queries/advanced/*.edn` (43 queries: 32 supported, 3 supported-with-warning, 8 rejected).
- Docs: `docs/design/sqlite-index-schema.md` §7.5 and §8.1.

## Verification
`cargo fmt --all`, `cargo clippy -p bitacora-index --all-targets --locked -- -D warnings` (clean), `cargo test -p bitacora-index --locked` (all green: 38 lib + integration suites, 17 simple-DSL, 3 advanced), `typos`, `cargo xtask check-deps` OK.

## Open / pending
Black-box comparison against Logseq (BIT-T-0301, BIT-T-0308) needs the Logseq runtime; expected results were derived from the documented semantics. Deviations: case-insensitive text/property values; `not` with several clauses negates their conjunction; empty aggregates return one row; user `:rules`, reverse attributes, `:result-transform`, `:view` unsupported; `:namespace`/`:alias` direct only.
