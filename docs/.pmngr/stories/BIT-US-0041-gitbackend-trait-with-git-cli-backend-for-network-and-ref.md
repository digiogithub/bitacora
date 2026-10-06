---
id: BIT-US-0041
type: story
title: GitBackend trait with git CLI backend for network and ref writes
status: done
priority: critical
parent: BIT-EP-0011
milestone: BIT-M-0004
author: mcp
labels: [git, sync, backend]
estimate: 8
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T17:30:22Z
closed: 2026-10-06T17:30:22Z
---

## Description
As a user syncing my graph, I want Bitacora to use my installed git for fetch/push/commit when I have one, so that my SSH keys, `~/.ssh/config`, credential helpers and Git Credential Manager just work on every OS — and to keep working without git installed.

Context: ADR-007 hybrid backend, amended by ADR-020 (git is not bundled). `GitBackend` trait (`fetch`, `push`, `merge_base`, `read_blob`, `diff_trees`, `write_tree`, `commit`, `update_ref`, `status`) with `CliBackend` for network/ref-writing ops when system git is present; otherwise the gix-only backend (BIT-US-0042) serves every op.

## Acceptance Criteria
- `CliBackend` runs `fetch`, `push`, `commit`, `commit-tree`, `update-ref`, `clone`, `ls-remote` with `GIT_TERMINAL_PROMPT=0`, `GIT_ASKPASS`, `LC_ALL=C`, `-c core.quotepath=false`, `-c core.autocrlf=false`.
- Errors are classified (`Network`, `Auth`, `NonFastForward`, `GitTooOld`, `NotARepo`, `Other`) from exit code + stderr.
- Backend detection at startup: system git ≥ 2.38 (setting `sync.git_binary`, else `PATH`) → CLI hybrid backend; absent or too old → gix-only backend. No bundled MinGit on any OS. The selected backend is exposed in `SyncStatus` and `bitacora-cli doctor`.
- Never writes global git config; never force-pushes.
- Integration tests against temp bare repos.

## Notes
Implements: BIT-SP-0006.R6, BIT-SP-0006.R3, BIT-SP-0006.R4. See [[git-sync-merge]] §3, [[05-git-and-apis]] §1. ADR-007, ADR-020.
