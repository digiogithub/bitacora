---
id: BIT-T-0038
type: task
title: delete_file, built-in page seeding and placeholder GC
status: backlog
priority: high
parent: BIT-US-0006
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 2
created: 2026-10-06T14:27:35Z
updated: 2026-10-06T15:11:55Z
---

## Description
`crates/bitacora-index/src/writer/gc.rs`: seed built-in pages (`TODO`, `DOING`, `DONE`, `LATER`, `NOW`, `WAIT`, `WAITING`, `CANCELED`, `CANCELLED`, `IN-PROGRESS`, `A`, `B`, `C`, `Contents`, `Favorites`, `card`) with `is_builtin = 1` on schema creation. `delete_file(path)`: steps 0, 2, 3 (demote unconditionally), delete `files` row (cascading to its derived rows; there is no snapshot row, ADR-017), GC. GC query as in §4.4 step 10 (no refs, no property refs, no alias/tag rows, no namespace children, no blocks).

## Acceptance Criteria
- Tests matching BIT-SP-0003.R5 scenarios: placeholder `zeta` removed; deleted `beta.md` keeps `beta` as placeholder; `todo` never deleted.
- Namespace parent placeholder `work` is kept while `work/q3` exists.

## Notes
BIT-SP-0003.R5. ADR-017. Logseq `retract-page-attributes` (`deps/db/src/logseq/db/schema.cljs:131-142`).
