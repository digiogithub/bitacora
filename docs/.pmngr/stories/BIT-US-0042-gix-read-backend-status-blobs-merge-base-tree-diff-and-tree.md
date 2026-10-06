---
id: BIT-US-0042
type: story
title: "gix read backend: status, blobs, merge-base, tree diff and tree writing"
status: backlog
priority: high
parent: BIT-EP-0011
milestone: BIT-M-0004
author: mcp
labels: [git, sync, backend, gix]
estimate: 8
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T15:14:45Z
---

## Description
As the sync engine, I want in-process git reads and tree building via `gix`, so that merges over hundreds of files don't spawn hundreds of git processes and stay pure-Rust testable. When no system git is installed, the same `GixBackend` must also perform fetch, push, clone, commit and ref updates so sync works without git (ADR-020).

## Acceptance Criteria
- `GixBackend` implements `status` (dirty paths, unmerged entries with stages 1/2/3), `read_blob(commit, path)`, `merge_base(a, b)`, `diff_trees(a, b)` with rename detection (≥ 50% similarity), and `write_tree(base_tree, edits)` returning a tree id.
- `GixBackend` also implements `fetch`, `push` (never forced), `clone`, `ls-remote`, `commit`, `commit_tree` and `update_ref` (CAS), so it can run as the only backend (BIT-T-0373).
- Results match `git` CLI output on a fixture matrix of repos (property-style comparison test).
- gix is pinned with an exact version in `[workspace.dependencies]`; API usage isolated in one module.

## Notes
Implements: BIT-SP-0006.R6, BIT-SP-0006.R19. See [[git-sync-merge]] §3, §5.5, [[crate-stack]]. ADR-007, ADR-020.
