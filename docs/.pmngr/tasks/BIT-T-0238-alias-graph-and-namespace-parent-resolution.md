---
id: BIT-T-0238
type: task
title: Alias graph and namespace parent resolution
status: done
priority: high
parent: BIT-US-0098
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat]
estimate: 2
created: 2026-10-06T14:31:34Z
updated: 2026-10-06T17:31:40Z
closed: 2026-10-06T17:31:40Z
---

## Description
`crates/bitacora-core/src/model/{alias.rs,namespace.rs}`:
- Alias: for each page with `alias`/`aliases`, create/attach virtual alias pages linked both ways (`extract.cljc:65-103`); drop blank aliases and aliases equal to the page's own key; `resolve(key)` follows alias links to the canonical file-backed page (cycle-safe).
- Namespace: `is_namespace(title)` per `graph_parser/text.cljs:79-85` (contains `/`, not starting with `./` or `../`, not a URL); register virtual parents `a`, `a/b` for `a/b/c` with `namespace_parent` pointing to the direct parent (`extract.cljc:194-199`, `util.cljs:116-125`); children index for the namespace view and rename cascade.

## Acceptance Criteria
- Alias scenario from BIT-SP-0002.R18 passes with no file created.
- `a/b/c` → virtual `a`, `a/b`; `./x/y` and `https://x.y/z` not namespaces.
- Alias cycles (`A alias B`, `B alias A`) do not loop.

## Notes
Refs BIT-SP-0002.R18, BIT-SP-0002.R12.
