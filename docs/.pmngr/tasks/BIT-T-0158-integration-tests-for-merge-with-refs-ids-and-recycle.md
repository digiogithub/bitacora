---
id: BIT-T-0158
type: task
title: Integration tests for merge with refs, ids and recycle
status: backlog
parent: BIT-US-0087
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, test, rename]
estimate: 2
created: 2026-10-06T14:30:30Z
updated: 2026-10-06T14:30:30Z
---

## Description
`crates/bitacora-core/tests/merge_pages.rs` on a temp graph: pages `Foo` (with `alias:: F2`, two blocks, one referenced by `((uuid))` from a journal), `Bar` (one block), journal refs `[[Foo]]`, `#Foo`. Run merge, assert file bytes, recycle path `logseq/.recycle/pages_Foo.md`, ref rewrite, and that undo restores the exact pre-merge tree.

## Acceptance Criteria
- All assertions above pass on Linux/macOS/Windows CI.

## Notes
BIT-SP-0002.R13, R14.
