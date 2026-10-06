---
id: BIT-T-0347
type: task
title: Merge matrix fixture tests (base/ours/theirs/expected)
status: done
priority: high
parent: BIT-US-0069
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, test, merge]
estimate: 3
created: 2026-10-06T14:34:14Z
updated: 2026-10-06T19:42:48Z
closed: 2026-10-06T19:42:48Z
---

## Description
Add the external-edit cases to the shared golden merge matrix `fixtures/merge/<case>/{base,ours,theirs,expected}.md` (or `expected.conflict`) run by `crates/bitacora-merge/tests/merge_matrix.rs` (ADR-016), and add `crates/bitacora-core/tests/merge3_external.rs` that runs the same cases through the core adapter (ops applied to the in-memory page, then serialized). Cases: disjoint edits, both append, insert at same anchor, edit vs delete, delete vs delete, move vs edit, move vs move, indent vs edit, `collapsed::`-only both sides, `id::` added on one side, CRLF base, spaces indentation; plus the no-base fallback (ADR-017).

## Acceptance Criteria
- ≥ 15 cases; expected output byte-exact for clean merges, both in `bitacora-merge` and through the core adapter.
- Property test: merge3(base, base, ours) == ours and merge3(base, theirs, base-as-ours) == theirs.

## Notes
Story BIT-US-0069. Verifies BIT-SP-0005.R15. AGENTS.md §6 merge matrix style. ADR-016, ADR-017. Shares fixtures with BIT-T-0370.
