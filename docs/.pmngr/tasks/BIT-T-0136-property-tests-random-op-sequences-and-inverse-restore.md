---
id: BIT-T-0136
type: task
title: "Property tests: random op sequences and inverse restore exact serialized bytes"
status: backlog
priority: high
parent: BIT-US-0029
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, test]
estimate: 3
created: 2026-10-06T14:30:00Z
updated: 2026-10-06T14:30:00Z
---

## Description
Add `crates/bitacora-core/tests/op_invariants.rs` using `proptest`: generate random valid op sequences (insert/remove/move/set/edit/adopt) over pages loaded from `fixtures/graphs/**`; after each commit check invariants; then apply inverses in reverse and assert `serialize(page) == original bytes`. Include a shrinking-friendly op generator that only emits valid targets.

## Acceptance Criteria
- ≥ 1,000 cases per fixture run in CI under 60 s total.
- Failure output prints the minimal op sequence and the fixture path.
- Test covers CRLF, spaces-indented and `*`-bullet fixtures.

## Notes
Story BIT-US-0029. Verifies BIT-SP-0004.R4, BIT-SP-0004.R5. AGENTS.md §6 (op/undo invariants).
