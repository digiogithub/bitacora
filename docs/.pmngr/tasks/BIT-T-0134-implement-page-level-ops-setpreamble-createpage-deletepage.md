---
id: BIT-T-0134
type: task
title: "Implement page-level Ops: SetPreamble, CreatePage, DeletePage, RenameFile"
status: done
priority: high
parent: BIT-US-0029
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, core]
estimate: 2
created: 2026-10-06T14:30:00Z
updated: 2026-10-06T18:39:25Z
started: 2026-10-06T18:28:39Z
closed: 2026-10-06T18:39:25Z
---

## Description
Add page-level variants to `Op` in `crates/bitacora-core/src/ops.rs`: `SetPreamble { page, before, after }`, `CreatePage { page, title, path }` (virtual page if `path` is None), `DeletePage { page, captured }` (captures a `PageSnapshot`), `RenameFile { page, from, to }` (model-only; the writer performs the filesystem rename). Each has an inverse.

## Acceptance Criteria
- Apply/inverse round-trip tests for each variant.
- `DeletePage` inverse restores blocks, origins and disk snapshot.
- `RenameFile` marks the page so the writer renames before writing.

## Notes
Story BIT-US-0029. Implements BIT-SP-0004.R5. Page rename cascade itself belongs to BIT-EP-0009.
