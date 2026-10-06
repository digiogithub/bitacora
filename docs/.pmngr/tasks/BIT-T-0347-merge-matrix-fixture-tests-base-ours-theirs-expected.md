---
id: BIT-T-0347
type: task
title: Merge matrix fixture tests (base/ours/theirs/expected)
status: backlog
priority: high
parent: BIT-US-0069
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, test, merge]
estimate: 3
created: 2026-10-06T14:34:14Z
updated: 2026-10-06T14:34:14Z
---

## Description
Add `fixtures/merge3/<case>/{base,ours,theirs,expected}.md` (or `expected.conflict`) and `crates/bitacora-core/tests/merge3_matrix.rs`. Cases: disjoint edits, both append, insert at same anchor, edit vs delete, delete vs delete, move vs edit, move vs move, indent vs edit, `collapsed::`-only both sides, `id::` added on one side, CRLF base, spaces indentation.

## Acceptance Criteria
- ≥ 15 cases; expected output byte-exact for clean merges.
- Property test: merge3(base, base, ours) == ours and merge3(base, theirs, base-as-ours) == theirs.

## Notes
Story BIT-US-0069. Verifies BIT-SP-0005.R15. AGENTS.md §6 merge matrix style.
