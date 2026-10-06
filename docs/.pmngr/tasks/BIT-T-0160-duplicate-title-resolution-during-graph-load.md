---
id: BIT-T-0160
type: task
title: Duplicate-title resolution during graph load
status: backlog
priority: high
parent: BIT-US-0088
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat]
estimate: 2
created: 2026-10-06T14:30:36Z
updated: 2026-10-06T14:30:36Z
---

## Description
In `crates/bitacora-core/src/graph/load.rs`, when building the page table from scanned files in `filter-files` order, the first file mapping to a `PageKey` wins; later files are skipped and recorded as `Diagnostic::DuplicateTitle { key, kept: GraphPath, skipped: GraphPath }` with Logseq's message "The file X will be skipped because another file Y has the same page title" (`handler/repo.cljs:216-238`). Skipped files remain untouched on disk and are excluded from refs. Expose diagnostics to the UI/CLI (`bitacora-cli doctor`).

## Acceptance Criteria
- Fixture `fixtures/graphs/duplicates/` (`pages/Foo.md`, `pages/sub/foo.md`, `pages/x.md` with `title:: foo`): exactly one page `foo`, two diagnostics, deterministic winner across OSes.
- Journal duplicate (`journals/2024_01_01.md` vs `pages/2024_01_01.md`): journals win (ordered first).

## Notes
Refs BIT-SP-0002.R8. Runtime case-only rename handling belongs to EP-0009.
