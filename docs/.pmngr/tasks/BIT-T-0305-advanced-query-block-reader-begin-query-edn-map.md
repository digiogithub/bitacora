---
id: BIT-T-0305
type: task
title: Advanced query block reader (BEGIN_QUERY EDN map)
status: done
priority: medium
parent: BIT-US-0103
milestone: BIT-M-0005
author: mcp
labels: [bitacora-index, query]
estimate: 2
created: 2026-10-06T14:33:07Z
updated: 2026-10-06T19:05:56Z
started: 2026-10-06T19:05:49Z
closed: 2026-10-06T19:05:56Z
---

## Description
`crates/bitacora-index/src/query/datalog/read.rs`: parse `#+BEGIN_QUERY … #+END_QUERY` bodies into `AdvancedQuery { title, query: DatalogQuery, inputs, collapsed, result_transform: Option<Raw>, view: Option<Raw>, rules: Vec<Rule> }`; `DatalogQuery` AST for `:find`, `:in`, `:where` clauses (data patterns, predicates, fn bindings, `not`, `not-join`, `or`, `or-join`, rule calls).

## Acceptance Criteria
- Parses all queries in the real-world corpus file `fixtures/queries/advanced/*.edn` without panics; errors carry line/column.

## Notes
BIT-SP-0003.R19. [[03-parsing-indexing-search]] §8.2.
