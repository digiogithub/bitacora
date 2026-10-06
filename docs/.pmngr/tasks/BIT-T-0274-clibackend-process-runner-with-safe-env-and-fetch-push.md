---
id: BIT-T-0274
type: task
title: "CliBackend: process runner with safe env and fetch/push/commit/commit-tree/update-ref"
status: backlog
priority: critical
parent: BIT-US-0041
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, git, backend]
estimate: 3
created: 2026-10-06T14:32:56Z
updated: 2026-10-06T14:32:56Z
---

## Description
`crates/bitacora-sync/src/backend/cli.rs`: `GitCommand` builder running `git -C <graph> -c core.quotepath=false -c core.autocrlf=false <args>` with env `GIT_TERMINAL_PROMPT=0`, `GIT_ASKPASS`/`SSH_ASKPASS=<askpass helper>`, `SSH_ASKPASS_REQUIRE=force`, `LC_ALL=C`, `GIT_OPTIONAL_LOCKS=0` for reads; timeout per op (fetch/push 120 s), kill on timeout. Implement `fetch` (`fetch --prune --no-tags origin <branch>`), `push` (`push origin HEAD:<branch>`, never `--force*`; parse `[rejected]`/`non-fast-forward`), `commit` (`commit -m` via `-F -` stdin, `--allow-empty=false`, `--amend` when requested), `commit_tree` (`commit-tree <tree> -p A -p B -F -`), `update_ref` (`update-ref <ref> <new> <old>`), `clone`, `ls-remote`. Classify stderr into `GitError`.

## Acceptance Criteria
- Unit tests for stderr classification with captured samples (ssh auth failure, DNS failure, non-fast-forward, index.lock).
- Never invokes `git config --global`.

## Notes
Story BIT-US-0041. Implements BIT-SP-0006.R6, BIT-SP-0006.R4. ADR-007.
