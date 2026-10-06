---
id: BIT-T-0306
type: task
title: "Datalog-to-SQL compiler: typed variables, relations and joins"
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
`crates/bitacora-index/src/query/datalog/compile.rs`: map each attribute to its relation per [[sqlite-index-schema]] §8 table; infer each variable's type (page/block/scalar) from attribute usage, reject ambiguous vars with a clear error; generate one SQL alias per data pattern, equalities for shared vars, constants as bound params. Support `:block/parent` typed union, `:block/refs` (page refs ∪ block refs joined by uuid), `:block/path-refs` via the view, `:block/properties` fused with `(get ?p :k)`.

## Acceptance Criteria
- Unit tests: 15 queries of increasing complexity compile and return expected rows on a fixture graph.
- Ambiguous variable test returns `TypeError { var }`.

## Notes
BIT-SP-0003.R19.
