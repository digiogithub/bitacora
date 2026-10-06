---
id: BIT-T-0361
type: task
title: Add/add page merge with empty-template rule and lazy journal file creation
status: backlog
priority: high
parent: BIT-US-0052
milestone: BIT-M-0004
author: mcp
labels: [bitacora-merge, bitacora-core, merge, journals]
estimate: 2
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T15:10:04Z
---

## Description
In `crates/bitacora-merge/src/page.rs`: when base is absent, treat as empty page and union blocks (ours first, dedupe). `is_trivial_page(bytes, cfg)` true for empty, `-`, `*`, whitespace, or content equal to the configured default journal template (`:default-templates {:journals …}` rendered) → that side counts as unchanged (Logseq `watcher_handler.cljs:95-101`). In `bitacora-core`: verify/ensure today's journal is virtual and no file is written until first non-empty edit (coordinate with core journal service; add test if behaviour exists).

## Acceptance Criteria
- Tests for both scenarios of BIT-SP-0006.R18; core test: opening today without typing creates no file.

## Notes
Story BIT-US-0052. Implements BIT-SP-0006.R18. See [[05-git-and-apis]] §1.5.
ADR-016: lives in the `bitacora-merge` crate (depends only on `bitacora-markdown`), shared by `bitacora-core` (external edits, BIT-US-0069) and `bitacora-sync`.
