---
id: BIT-T-0281
type: task
title: Enable sync on existing graph (init/connect) and open graph from remote (clone)
status: done
priority: high
parent: BIT-US-0043
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, git, onboarding]
estimate: 3
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T17:30:22Z
closed: 2026-10-06T17:30:22Z
---

## Description
`crates/bitacora-sync/src/onboarding.rs`: `enable_sync(graph, remote_url, branch)`: if no `.git` → `git init -b main`, add remote, prepare repo, initial commit (`Kind: migrate`), first push (`-u`); if `.git` exists → validate remote, prepare repo. If remote has history and local doesn't share it → fetch and run merge with empty base (add/add policy). `clone_graph(url, dest)` → `git clone`, prepare repo, open graph. Detect Logseq separate gitdir (`.git` is a file pointing into `~/.logseq/git/`) and offer migration: copy that gitdir into `<graph>/.git`, remove `core.worktree`, verify `git status`.

## Acceptance Criteria
- Integration tests with temp bare repos: empty remote, non-empty remote, separate-gitdir migration.

## Notes
Story BIT-US-0043. Implements BIT-SP-0006.R3. See [[05-git-and-apis]] §1.1, Open question 4.
