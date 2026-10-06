---
created_at: 2026-10-06T21:08:41.221873296Z
updated_at: 2026-10-06T21:08:41.221873296Z
tags:
    - change
    - agent-guidance
    - disk
---
# Rule: remove git worktrees once merged or validated

## What changed
AGENTS.md §9 gained a MANDATORY rule: after a worktree branch is merged into `main` (or validated and discarded) remove it (`git worktree remove --force`, `git branch -D`, `git worktree prune`) and check `git worktree list` at the end of every task. The orchestrator's merge helper now removes the agent worktree right after a clean merge.

## Why
Each worktree has its own Rust `target/` (GPUI builds 5–25 GB). On 2026-10-06 ~45 finished agent worktrees under `.claude/worktrees/` took ~345 GB; free disk fell from ~300 GB to 60 GB. Removing merged worktrees recovered it (387 GB free).

## Files
- `AGENTS.md` §9.

## Verification
`git worktree list` shows only active agents; `df -h /` 60 GB → 387 GB free.

Links: [[bitacora-full-development-plan]]
