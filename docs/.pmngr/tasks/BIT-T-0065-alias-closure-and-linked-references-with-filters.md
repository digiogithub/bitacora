---
id: BIT-T-0065
type: task
title: Alias closure and linked references with filters
status: done
priority: high
parent: BIT-US-0008
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 3
created: 2026-10-06T14:28:35Z
updated: 2026-10-06T18:39:52Z
closed: 2026-10-06T18:39:52Z
---

## Description
`crates/bitacora-index/src/read/refs.rs`: `alias_closure(page_id)` (recursive CTE, symmetric, 2 hops), `linked_references(page_id, filters) -> Vec<RefGroup { page, blocks: Vec<RefHit{ block, breadcrumb }> }>` using path-refs ∩ alias set, excluding `d.page_id = page_id`, keeping only the top-most match per subtree, grouped by page (journals newest first, then pages by name). Support `filters::` page property (`{"include" ...}` / `{"exclude" ...}` map as Logseq stores it) as include/exclude predicates on `block_path_refs`. Also `alias_redirect(page)` returning the source page for empty alias pages.

## Acceptance Criteria
- Tests matching BIT-SP-0003.R9 scenarios (inherited ref, alias closure, own page excluded).
- Filter test: excluding `[[meeting]]` removes blocks whose path-refs contain `meeting`.
- Linked references count for each fixture page equals the expected count exported from Logseq.

## Notes
BIT-SP-0003.R9. Logseq `model.cljs:1255-1283`, `rules.cljc:14-24`.
