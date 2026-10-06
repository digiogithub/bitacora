---
id: BIT-T-0298
type: task
title: "DSL parser: pre-transform, EDN reader and query AST"
status: backlog
priority: high
parent: BIT-US-0101
milestone: BIT-M-0005
author: mcp
labels: [bitacora-index, query]
estimate: 3
created: 2026-10-06T14:33:07Z
updated: 2026-10-06T14:33:07Z
---

## Description
`crates/bitacora-index/src/query/dsl/parse.rs`: apply Logseq's pre-transform (`[[x]]` → page ref, `#x` → tag ref, `between` args → keywords; `query_dsl.cljs:452-476`), read the result as an EDN s-expression (`edn-format` crate or a small hand-written reader), and build `enum Q { And(Vec<Q>), Or(Vec<Q>), Not(Vec<Q>), PageRef(String), Text(String), Task(BTreeSet<Marker>), Priority(BTreeSet<char>), Property(String, Option<Value>), PageProperty(String, Option<Value>), Between(DateArg, DateArg), BetweenProp(String, DateArg, DateArg), Page(String), Namespace(String), PageTags(Vec<String>), AllPageTags, SortBy(String, Dir), Sample(u32) }`. A bare string or page ref at top level is accepted as in Logseq. Values parsed per `query_dsl.cljs:242-263` (int/bool, `#x`→x, `[[x]]`→x).

## Acceptance Criteria
- Unit tests for every form, nested boolean logic, and malformed input (error with byte position).
- Proptest: printing then re-parsing the AST is identity.

## Notes
BIT-SP-0003.R18. [[sqlite-index-schema]] §7.1 step 1.
