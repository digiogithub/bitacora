---
id: BIT-T-0351
type: task
title: "Three-way block matcher: id, exact, LCS and fuzzy passes"
status: in_progress
priority: critical
parent: BIT-US-0049
milestone: BIT-M-0003
author: mcp
labels: [bitacora-merge, merge]
estimate: 3
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T17:08:27Z
started: 2026-10-06T17:08:27Z
---

## Description
`crates/bitacora-merge/src/matcher.rs`: `match_blocks(B, O, T) -> Vec<Triple{b, o, t: Option<NodeRef>}>`. Pass 1: equal `id::` (global across tree, so moves are tracked). Pass 2: per matched parent, exact normalized content. Pass 3: LCS over remaining children by content hash. Pass 4: fuzzy — candidates under same or matched-moved parent, score = max(token Jaccard, 1 - normalized Levenshtein on first line via `strsim`), threshold 0.6, greedy best-first one-to-one, with a minimum length guard (blocks < 4 tokens require exact match) to avoid mis-pairing "- TODO". Duplicate `id::` in a file → second occurrence re-keyed with fresh uuid + log. Pairwise matching B↔O and B↔T, then O↔T for added-on-both blocks.

## Acceptance Criteria
- Unit tests for each pass, including the scenarios in BIT-SP-0006.R9 and short-block guard.

## Notes
Story BIT-US-0049. Implements BIT-SP-0006.R9. See [[git-sync-merge]] §4.2, open question 3.
ADR-016: lives in the `bitacora-merge` crate (depends only on `bitacora-markdown`), shared by `bitacora-core` (external edits, BIT-US-0069) and `bitacora-sync`.
Needed by BIT-US-0069 (ADR-016).
