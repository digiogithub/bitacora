---
id: BIT-US-0041
type: story
title: GitBackend trait with git CLI backend for network and ref writes
status: backlog
priority: critical
parent: BIT-EP-0011
milestone: BIT-M-0004
author: mcp
labels: [git, sync, backend]
estimate: 8
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T14:28:30Z
---

## Description
As a user syncing my graph, I want Bitacora to use my installed git for fetch/push/commit, so that my SSH keys, `~/.ssh/config`, credential helpers and Git Credential Manager just work on every OS.

Context: ADR-007 hybrid backend. `GitBackend` trait (`fetch`, `push`, `merge_base`, `read_blob`, `diff_trees`, `write_tree`, `commit`, `update_ref`, `status`) with `CliBackend` for network/ref-writing ops.

## Acceptance Criteria
- `CliBackend` runs `fetch`, `push`, `commit`, `commit-tree`, `update-ref`, `clone`, `ls-remote` with `GIT_TERMINAL_PROMPT=0`, `GIT_ASKPASS`, `LC_ALL=C`, `-c core.quotepath=false`, `-c core.autocrlf=false`.
- Errors are classified (`Network`, `Auth`, `NonFastForward`, `GitTooOld`, `NotARepo`, `Other`) from exit code + stderr.
- git ≥ 2.38 detected at startup; on Windows bundled MinGit is preferred unless system git is newer.
- Never writes global git config; never force-pushes.
- Integration tests against temp bare repos.

## Notes
Implements: BIT-SP-0006.R6, BIT-SP-0006.R3, BIT-SP-0006.R4. See [[git-sync-merge]] §3, [[05-git-and-apis]] §1. ADR-007.
