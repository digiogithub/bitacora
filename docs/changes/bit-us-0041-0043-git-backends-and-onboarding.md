---
created_at: 2026-10-06T17:30:31.857451231Z
updated_at: 2026-10-06T17:30:31.857451231Z
tags:
    - change
    - sync
---
# BIT-US-0041/0042/0043: GitBackend, CLI + gix backends, repo setup, onboarding

Tags: change, sync

Commit 8645f56 (worktree branch). See [[bitacora-full-development-plan]], [[git-sync-merge]], [[05-git-and-apis]].

## What
- `crates/bitacora-sync/src/backend/`: `GitBackend` trait, `GitError` (classified), `CliBackend` (non-interactive env, timeouts, never-forced push, CAS update-ref, ETXTBSY retry), `detect_git` (>= 2.38), `select_backend`, `HybridBackend`, `FakeBackend`, `GixBackend` (gix_read: status/blob/merge-base; gix_trees: rename-aware diff + id:: strong signal, write_tree; gix_net: commit/commit_tree/update_ref/fetch/clone/local push).
- `repo_setup.rs`: `ensure_gitignore`, `ensure_attributes`, `ensure_identity`, `prepare_repo`.
- `onboarding.rs`: `enable_sync`, `clone_graph`, `migrate_separate_gitdir`.
- `bitacora-testkit/src/git.rs` helpers; root Cargo.toml gix features += sha1, worktree-mutation.

## Gaps
- gix 0.88 has no push client: gix-only push works for local remotes only (documented in git-sync-merge open question 8). HTTPS credential provider (BIT-T-0374) not done. BIT-T-0282 (UI) not done.

## Verification
cargo test -p bitacora-sync: 14 unit + 17 backend + 6 onboarding tests pass; clippy -D warnings clean (sync, testkit); cargo xtask check-deps, cargo deny check, cargo machete OK.
