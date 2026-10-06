---
id: BIT-T-0370
type: task
title: Golden merge matrix fixtures and runner
status: done
priority: high
parent: BIT-US-0055
milestone: BIT-M-0004
author: mcp
labels: [bitacora-merge, merge, testing]
estimate: 3
created: 2026-10-06T14:34:50Z
updated: 2026-10-06T19:34:06Z
closed: 2026-10-06T19:34:06Z
---

## Description
`fixtures/merge/<case>/{base.md?, ours.md, theirs.md, expected.md, conflicts.json?}` and `crates/bitacora-merge/tests/merge_matrix.rs` iterating cases: metadata-only (collapsed, id add, LOGBOOK, card-*, property order, whitespace/indent, reorder), content (diff3 clean, same-line conflict, property conflict), structure (insert both, delete vs modify, child under deleted parent, competing moves), markers/tasks, add/add journal (+ template side), id clash with ref rewrite, CRLF/tab/BOM style. Runner compares bytes and conflict kinds/keys; `UPDATE_GOLDEN=1` to regenerate with review.

## Acceptance Criteria
- ≥ 30 cases, all green; each BIT-SP-0006 merge requirement scenario has a fixture.

## Notes
Story BIT-US-0055. Verifies BIT-SP-0006.R9–R14, R18.
ADR-016: lives in the `bitacora-merge` crate (depends only on `bitacora-markdown`), shared by `bitacora-core` (external edits, BIT-US-0069) and `bitacora-sync`. The same fixtures are also run through the core adapter (BIT-T-0347).
