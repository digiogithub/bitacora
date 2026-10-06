---
created_at: 2026-10-06T17:33:30.027613956Z
updated_at: 2026-10-06T17:33:30.027613956Z
tags:
    - change
    - core
    - compat
---
# Core: graph domain model, page identity, ignore-rules and journals fixtures

Commit a917efa. Continues [[bitacora-full-development-plan]] and [[bit-us-0081-core-naming-journal-scan]]; uses [[bit-us-0083-0084-0059-inline-scanner-tasks-page-props]]. Spec [[01-file-graph-layout]] §3, §4, §9; BIT-SP-0002.R8/R12/R18/R20.

## What changed
- `crates/bitacora-core/src/graph.rs` (new): `Graph::{load,from_texts,page,resolve,pages,alias_group,namespace_children,diagnostics}`, `Page`, `PageKey` (page_name_sanity_lc), `BlockId` (uuid), `Block` (type only), `PageFormat`, `PageOrigin` (File/Alias/Tag/Namespace), `Diagnostic::{DuplicateTitle,Unreadable}`.
- Titles come from bitacora-markdown `page_properties().title()` (front matter/directives beat pre-block) -> `derive_title` -> `detect_journal`. Props are normalised-key -> raw value; alias/aliases/tags interpreted with `interpret()` (comma splitting, config `:property/separated-by-commas`); blank and self aliases dropped.
- Alias links are symmetric (owner<->alias, alias<->alias); an alias with no file resolves to its declarer. Namespace parents created as virtual pages via `namespace_parents` (`./x`, `../x`, URLs excluded). Virtual pages never have files.
- Duplicate keys: first in `parse_order` wins, `DuplicateTitle{key,kept,skipped}`. `.org` pages `read_only`; non-page extensions (`.adoc`) ignored.
- Fixtures: `fixtures/graphs/ignore-rules/` and `fixtures/graphs/journals/` (+ PROVENANCE.md, manifest updated). `scan.rs` golden/hidden/order/no-trace tests now use ignore-rules (symlink and NFD tests keep tempdirs; `.DS_Store` is gitignored so only unit-tested).
- `crates/bitacora-core/tests/graph_fixtures.rs`: zero-write load over all 5 fixture graphs, journals under several title formats, ignore-rules, edge-cases namespaces.

## Not done
Block loading, referenced-page virtual pages, org body refs (index/editor layers).

## Verification
fmt; clippy -D warnings (core + deps) clean; `cargo test -p bitacora-core --locked`: 42 unit + 5 integration pass; markdown/config tests still pass; `cargo xtask fixtures verify` OK (429 files); check-deps OK.
