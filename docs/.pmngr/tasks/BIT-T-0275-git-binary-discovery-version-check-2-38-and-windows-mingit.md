---
id: BIT-T-0275
type: task
title: Git binary discovery, version check (>= 2.38) and Windows MinGit selection
status: backlog
priority: high
parent: BIT-US-0041
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, git, packaging]
estimate: 2
created: 2026-10-06T14:32:56Z
updated: 2026-10-06T14:32:56Z
---

## Description
`crates/bitacora-sync/src/backend/git_binary.rs`: resolve git from `sync.git_binary` setting, else bundled MinGit (Windows: `<app>/mingit/cmd/git.exe`), else `PATH`; on Windows prefer system git if its version is newer. Parse `git version 2.x.y(.windows.n)`. Below 2.38 → `GitError::GitTooOld`. Cache result; expose for `bitacora-cli doctor`. Document bundling step for the Windows packager (MinGit download + checksum) in [[crate-stack]].

## Acceptance Criteria
- Unit tests for version parsing incl. Apple Git and Windows suffixes.
- `bitacora-cli doctor` prints git path + version.

## Notes
Story BIT-US-0041. Implements BIT-SP-0006.R6. Architecture open decision: minimum git version / MinGit bundling.
