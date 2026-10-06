---
id: BIT-T-0352
type: task
title: Matcher accuracy benchmark on fixture graph edit pairs
status: backlog
priority: medium
parent: BIT-US-0049
milestone: BIT-M-0003
author: mcp
labels: [bitacora-merge, merge, testing]
estimate: 2
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T15:17:28Z
---

## Description
`crates/bitacora-merge/tests/matcher_accuracy.rs`: from `fixtures/graphs/**`, synthesize edits with known ground truth (word edits, line additions, moves, deletes, short task blocks) and measure precision/recall of the matcher; fail if precision < 0.99 or recall < 0.95. Report mis-pairs for tuning the 0.6 threshold; record outcome in [[git-sync-merge]] open question 3.

## Acceptance Criteria
- Test runs in < 30 s; thresholds asserted; doc note updated.

## Notes
Story BIT-US-0049. Verifies BIT-SP-0006.R9.
ADR-016: lives in the `bitacora-merge` crate (depends only on `bitacora-markdown`), shared by `bitacora-core` (external edits, BIT-US-0069) and `bitacora-sync`.
Needed by BIT-US-0069 (ADR-016).
