---
id: BIT-T-0276
type: task
title: CliBackend integration tests with temp bare repos
status: backlog
priority: high
parent: BIT-US-0041
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, git, testing]
estimate: 2
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T15:15:35Z
---

## Description
`crates/bitacora-sync/tests/cli_backend.rs` + `tests/support/repos.rs` (`TempRemote::new_bare()`, `TempClone::clone_from(&remote)` using `tempfile`, local `file://` URLs, repo-local identity): fetch/push round-trip; push rejected non-fast-forward classified as `NonFastForward`; commit-tree with two parents + update-ref with expected old value (CAS failure detected); amend of unpushed commit; assert `~/.gitconfig` untouched by running with `HOME` pointed at a temp dir and checking it stays empty.

## Acceptance Criteria
- Runs on Linux/macOS/Windows CI in < 10 s.
- The same scenarios run against the gix-only backend (shared `tests/support`, see BIT-T-0376).

## Notes
Story BIT-US-0041. Verifies BIT-SP-0006.R3, BIT-SP-0006.R4, BIT-SP-0006.R6. AGENTS.md §6. ADR-020.
