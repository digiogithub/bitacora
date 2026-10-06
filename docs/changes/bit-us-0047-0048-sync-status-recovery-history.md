---
created_at: 2026-10-06T19:52:16.577103932Z
updated_at: 2026-10-06T19:52:16.577103932Z
tags:
    - change
    - sync
    - runtime
---
# BIT-US-0047 / BIT-US-0048: sync status stream, startup recovery, conflict resolve command, page history and restore

Continues [[bitacora-full-development-plan]], [[git-sync-merge]], [[changes/bit-us-0044-0045-auto-commit-and-sync-engine]], [[changes/bit-us-0052-0053-file-merge-policies-and-conflict-state]], [[changes/bit-us-0067-runtime-crate-wiring]]. Commit e705923 on worktree branch.

## What changed
- bitacora-sync `state.rs`: `SyncStatus` + `behind`, `conflict_pages`; `user_message()`, `backend_hint()` (install git on gix auth failure, ADR-020), `can_retry()`; `SyncError::user_message()`; `INSTALL_GIT_HINT`.
- `recovery.rs` (new): stale `index.lock` (>10 min, no git process via /proc scan), `external_operation()`, `RecoveryReport`.
- `engine.rs`: `SyncEngine::recover()`, `abort_external_operation()` (rebase via `git rebase --abort`; merge/cherry-pick/revert drop state files + reset index, work tree untouched), `count_behind`; commands `Recover/Resolve/ResolvePage/AbortExternal` plus `EngineHandle` blocking helpers.
- `history.rs` (new): `page_history` (first-parent walk, follows renames, trailers), `version_text`, `diff_version`.
- bitacora-merge `diff.rs` (new, additive): `diff_pages` -> `PageDiff`/`BlockDiff`/`DiffKind` using `match_pages`; MetaOnly hidden via `visible(false)`.
- bitacora-runtime: `sync_ctl.rs` (`SyncWatch`, `SyncStatusView`, `BackendInfo`), `restore.rs` (`RestoreReport`, restore via core `Cmd` transactions, undo by inverse ops), `Session::{sync_watch, sync_view, recovery_report, resolve_conflict, resolve_conflict_page, abort_external_operation, page_history, history_version, history_diff, restore_blocks, restore_page, undo_restore}`; recovery runs before the engine thread starts; MCP `git_sync_status` now returns real `conflict_pages` and `behind`. Sync slot holds `Arc<EngineHandle>` so blocking resolve calls never hold the slot mutex (queue observer would deadlock).

## Verification
cargo fmt, clippy --workspace --all-targets -D warnings clean; typos clean; xtask check-deps OK. Tests: bitacora-sync (recovery 6, history 1 + all existing), bitacora-merge diff (5 unit), bitacora-runtime sync_status (2: status stream + restart recovery + resolve through session; history diff + undoable block/page restore), bitacora-mcp/cli green.

## Left open (UI for the app agent)
BIT-T-0294, BIT-T-0295 (settings page + CLI), BIT-T-0375, BIT-T-0297.
