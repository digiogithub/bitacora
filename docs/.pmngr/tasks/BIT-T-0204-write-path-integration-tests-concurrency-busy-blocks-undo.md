---
id: BIT-T-0204
type: task
title: "Write-path integration tests: concurrency, busy blocks, undo and agent commits"
status: backlog
priority: high
parent: BIT-US-0020
milestone: BIT-M-0003
author: mcp
labels: [bitacora-mcp, testing, write]
estimate: 3
created: 2026-10-06T14:31:00Z
updated: 2026-10-06T14:31:00Z
---

## Description
`crates/bitacora-mcp/tests/write_path.rs` over HTTP with a real core on a temp graph (+ temp git repo): (1) `expected_version` race between two clients — exactly one wins, other gets `CONFLICT`; (2) simulated editor focus on u1 → `BLOCK_BUSY`, sibling write succeeds; (3) stub sync conflict on u3 → `BLOCK_IN_CONFLICT`; (4) undo stack has one labelled entry per call; (5) after debounce the commit body has `Bitacora-Kind: agent` and `Bitacora-Agent: <client>` (using stub sync committer if engine not available).

## Acceptance Criteria
- Tests deterministic (no sleeps > 100 ms; use injectable clock).

## Notes
Story BIT-US-0020. Verifies BIT-SP-0007.R7, BIT-SP-0007.R9, BIT-SP-0007.R10.
