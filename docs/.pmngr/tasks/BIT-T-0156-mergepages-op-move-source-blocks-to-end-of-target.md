---
id: BIT-T-0156
type: task
title: "MergePages op: move source blocks to end of target"
status: done
parent: BIT-US-0087
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, rename]
estimate: 3
created: 2026-10-06T14:30:30Z
updated: 2026-10-06T19:30:10Z
closed: 2026-10-06T19:30:10Z
---

## Description
`crates/bitacora-core/src/lifecycle/merge.rs`: build a transaction for `RenamePlan::Merge { source, target }` (`page.cljs:568-616`): `Op::MoveBlocks` of every top-level source block (with subtree) to after the last top-level target block; moved blocks are serialized in canonical form at their new depth (indent unit from `:export/bullet-indentation`), keeping `id::` and all properties; target's existing bytes untouched. Source pre-block properties are not moved (aliases reported back to the caller). Then reuse the ref cascade (source → target) and `Op::DeletePage(source)` (recycle).

## Acceptance Criteria
- Unit test: Foo(2 blocks, one with child and `id::`) + Bar(1 block) → Bar file = Bar block + Foo blocks; ids intact.
- Block refs `((uuid))` to moved blocks still resolve after reindex.
- Undo restores both files and the recycle entry is removed.

## Notes
BIT-SP-0002.R13, R14. ADR-006.
