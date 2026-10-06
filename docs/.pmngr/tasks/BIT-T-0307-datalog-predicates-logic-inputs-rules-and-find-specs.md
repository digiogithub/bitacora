---
id: BIT-T-0307
type: task
title: Datalog predicates, logic, inputs, rules and find specs
status: done
priority: medium
parent: BIT-US-0103
milestone: BIT-M-0005
author: mcp
labels: [bitacora-index, query]
estimate: 5
created: 2026-10-06T14:33:07Z
updated: 2026-10-06T19:05:56Z
started: 2026-10-06T19:05:50Z
closed: 2026-10-06T19:05:56Z
---

## Description
Extend the compiler with: predicates `=`, `not=`, `<`, `<=`, `>`, `>=`, `contains?`, `get-else` (LEFT JOIN + COALESCE), `missing?` (NOT EXISTS), `clojure.string/includes?`/`starts-with?`/`ends-with?`, `re-find`+`re-pattern` via a `regexp()` UDF registered from the `regex` crate, `str`, `untuple`; `not`/`not-join` → NOT EXISTS, `or`/`or-join` → UNION subqueries; inline DSL rules from `rules.cljc:63-143`, recursive `namespace`/`alias` rules via `WITH RECURSIVE`; inputs `:current-page`, `:query-page`, `:current-block`, `:parent-block`, `:today`, `:±Nd`, `:±Nd-start`, `:today-HHMM`, `:right-now-ms`, `"[[page]]"`; find specs `?x`, `[?x ...]`, `(pull ?b [*])` (hydrate in Rust), `count`/`min`/`max`.

## Acceptance Criteria
- Tests for each predicate/logic form and each input keyword with a fixed clock.
- `regexp()` UDF has a size/time guard (regex compiled once per query, `RegexBuilder::size_limit`).

## Notes
BIT-SP-0003.R19. [[sqlite-index-schema]] §8.
