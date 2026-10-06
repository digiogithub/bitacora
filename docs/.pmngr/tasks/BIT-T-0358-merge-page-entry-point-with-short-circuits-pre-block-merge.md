---
id: BIT-T-0358
type: task
title: merge_page entry point with short-circuits, pre-block merge and parse-failure fallback
status: backlog
priority: high
parent: BIT-US-0051
milestone: BIT-M-0003
author: mcp
labels: [bitacora-merge, merge]
estimate: 2
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T15:17:52Z
---

## Description
`crates/bitacora-merge/src/page.rs`: `pub fn merge_page(b: Option<&[u8]>, o: &[u8], t: &[u8], ctx: &MergeCtx) -> MergeResult { output: Vec<u8>, conflicts: Vec<ConflictRecord>, notes: Vec<Note>, id_rewrites }`: byte-equality short-circuits; pre-block page properties merged with the same per-key rules (title/alias/tags content-class; filters metadata); orchestrates matcher → fields → meta → structure → emit; tolerant parse failure → `diff3_lines` fallback where overlap becomes a whole-file `Conflict::Content` (output ours). Post-merge side effects requested by ctx (ensure `id::` on blocks newly referenced by theirs) are applied here so they land in the merge commit.

## Acceptance Criteria
- Unit tests for short-circuits, pre-block merge, fallback path, id:: side effect included.

## Notes
Story BIT-US-0051. Implements BIT-SP-0006.R10, BIT-SP-0006.R14.
ADR-016: lives in the `bitacora-merge` crate (depends only on `bitacora-markdown`), shared by `bitacora-core` (external edits, BIT-US-0069) and `bitacora-sync`.
Needed by BIT-US-0069 (ADR-016).
