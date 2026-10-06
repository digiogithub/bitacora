---
created_at: 2026-10-06T17:49:20.757473957Z
updated_at: 2026-10-06T17:49:20.757473957Z
tags:
    - change
    - sync
---
# BIT-US-0044/0045: idle auto-commit, squashing and the sync loop state machine

Commit c308f55 (worktree branch). Continues [[bitacora-full-development-plan]]; designs [[git-sync-merge]] (new section 8); follows [[changes/bit-us-0041-0043-git-backends-and-onboarding.md]] and [[changes/bit-us-0051-structural-merge-and-page-serialization.md]].

## What changed (crates/bitacora-sync)
- `writer.rs`: `GraphWriter::acquire() -> GraphLock`, `GraphLock::apply(&[FileChange])` with `expected` content (stale = nothing applied), `WriterError::{Busy,Stale,Failed}`; `writer::testing::DirGraphWriter` fake. Core implements it later (BIT-US-0062); the engine never writes graph files itself.
- `commit_msg.rs`: `build_message(MessageSpec)`, `parse_message`, subject <= 72 chars, trailers Device/Kind/Pages/Agent.
- `autocommit.rs`: `Debouncer` (idle + hard cap, clamped 5 s..10 min), `is_ignored_path`, `commit_changes`/`commit_locked`, squash of unpushed `Kind: auto` commits (same device, < 30 min, `is_unpushed`).
- `merge.rs`: `plan_merge` (tree-level, per-path policies: markdown via `merge_page`, config/text via `merge_lines`, whiteboard/binary keep-both copies, blank-journal add/add, file delete-vs-modify, rename tracking, empty base for unrelated histories) -> `MergePlan { edits, changes, conflicts: Vec<ConflictRecord>, notes }`.
- `state.rs`: `SyncState`/`SyncError`, `can_transition` table of the design 2.5 diagram, `SyncStatus`, `Backoff` (30 s..10 min), `PendingMerge` + `MergeStateStore` trait + `MemoryMergeStore` (seam for BIT-US-0053).
- `engine.rs`: `SyncEngine` (commit -> fetch -> classify -> fast-forward / merge -> push with 5 attempts and 1-8 s jitter, offline back-off, writer-busy deferral, Conflicted handling incl. recompute when the remote moves, `finish_pending_merge` resolver seam, `refs/bitacora/pending-merge`), `Timing` trait, `spawn` thread + `Command` channel (`FileFlushed`, `SyncNow`, `NetworkUp`, `Focus`, `Close`, `Shutdown`).
- Backend: `GitBackend` gained `resolve_ref`, `commit_info` (`CommitInfo`), `reset_index`; `Oid::empty_tree()`; classifier treats a missing local remote as `Network`.

## Tests
- `tests/auto_commit.rs` (8) and `tests/sync_engine.rs` (18), each looped over hybrid (system git) and gix-only; temp bare remote, two clones, racing backend wrapper for push races. Plus unit tests (commit_msg, autocommit, state, writer, merge).

## Verification
`cargo test -p bitacora-sync --locked`: 37 unit + 8 + 17 backends + 6 onboarding + 18 sync_engine pass; `cargo clippy -p bitacora-sync -p bitacora-testkit --all-targets --locked -D warnings` clean; `cargo xtask check-deps`, `cargo deny check`, `cargo machete` OK.

## Open / follow-ups
- Persisting `PendingMerge` (merge-state.json), resolution memo and the resolver UI: BIT-US-0053/0054.
- Core must implement `GraphWriter` (BIT-US-0062); requirement R7 stays unverified until then.
- `MergeEnv.prefer` is always ours: `Side` is not re-exported by `bitacora-merge`.
- External marker import / unmerged index entries (R8) only report `ExternalOperationInProgress`.
- `verify_requirement` needs `gintrack spec ingest` of test results (not possible here).
