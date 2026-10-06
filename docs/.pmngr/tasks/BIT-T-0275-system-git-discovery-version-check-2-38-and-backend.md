---
id: BIT-T-0275
type: task
title: System git discovery, version check (>= 2.38) and backend selection
status: backlog
priority: high
parent: BIT-US-0041
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, git, backend]
estimate: 2
created: 2026-10-06T14:32:56Z
updated: 2026-10-06T15:15:35Z
---

## Description
`crates/bitacora-sync/src/backend/git_binary.rs`: resolve git from `sync.git_binary` setting, else `PATH` (no bundled MinGit on any OS, ADR-020). Parse `git version 2.x.y(.windows.n)`. Found and ≥ 2.38 → `Detection::SystemGit { path, version }`; not found → `Detection::None`; below 2.38 → `Detection::TooOld(version)` (logged, not an error). Feed the result to `select_backend` (BIT-T-0273): system git → hybrid CLI + gix; otherwise gix-only backend (BIT-T-0373). Re-detect on settings change and on "Retry". Cache result; expose for `bitacora-cli doctor` and `SyncStatus.backend` (BIT-T-0375).

## Acceptance Criteria
- Unit tests for version parsing incl. Apple Git and Windows suffixes.
- Unit tests for selection: found ≥ 2.38, too old, not found, invalid `sync.git_binary` path.
- `bitacora-cli doctor` prints git path + version, or "not found — using built-in gix backend".

## Notes
Story BIT-US-0041. Implements BIT-SP-0006.R6. ADR-020 resolves the former open decision on MinGit bundling (no bundling).
