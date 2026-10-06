---
id: BIT-T-0289
type: task
title: Two-clone convergence integration tests through a temp bare repo
status: done
priority: critical
parent: BIT-US-0045
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, testing]
estimate: 3
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T17:48:56Z
closed: 2026-10-06T17:48:56Z
---

## Description
`crates/bitacora-sync/tests/convergence.rs`: two graph clones (A, B) of a temp bare repo, each with a real `SyncEngine` + headless core. Scenarios: A edits → B fetches and fast-forwards; concurrent non-overlapping edits → merge commit, both converge to identical trees; push race (B pushes between A's fetch and push) → A retries and converges; remote unreachable (rename bare dir) → A Offline with local commits, then recovers; never a force push (bare repo reflog check). Deterministic via fake clock and explicit `SyncNow`. Written backend-agnostic so BIT-T-0376 can run it for both the system-git hybrid and the gix-only backend.

## Acceptance Criteria
- Final trees of A, B and bare are equal; no file contains marker lines.

## Notes
Story BIT-US-0045. Verifies BIT-SP-0006.R4, BIT-SP-0006.R5, BIT-SP-0006.R8. AGENTS.md §6. ADR-020.
