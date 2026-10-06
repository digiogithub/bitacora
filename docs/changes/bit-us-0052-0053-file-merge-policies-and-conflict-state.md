---
created_at: 2026-10-06T19:00:34.006386474Z
updated_at: 2026-10-06T19:00:34.006386474Z
tags:
    - change
    - sync
    - merge
---
# BIT-US-0052/0053: file-level merge policies, persisted conflict state, resolution API, marker guard

Commit ee89401 (worktree branch). Continues [[bitacora-full-development-plan]]; designs [[git-sync-merge]] (section 8 rewritten); follows [[changes/bit-us-0044-0045-auto-commit-and-sync-engine.md]] and [[changes/bit-us-0051-structural-merge-and-page-serialization.md]].

## What changed
- crates/bitacora-merge: `Side` re-exported (fixes `MergeEnv.prefer`), `MergeEnv.template`, `is_trivial_page` and the add/add rule in `merge_page` (page.rs), new `resolve.rs` (`Choice`, `resolve_conflict`, `ensure_block_ids`, `add_page_alias`).
- crates/bitacora-sync: `merge.rs` became `merge/` with `policy.rs` (`policy_for`), `edn.rs` (`merge_config`, `set_config_value`, via `bitacora-config`), `files.rs` (text diff3 with BOM/CRLF, copy names, `iso_date`), `renames.rs` (titles, `rewrite_links`, `block_refs`), `markers.rs` (`split_markers`); `plan_merge` extended (rename fix-up, delete-vs-modify restore, rename/rename alias suggestion, R14 id writes, memo, marker sanitising, device/date copy names, prefer by commit time). New `store.rs` (`JsonMergeStore`), `resolve.rs` (`plan_resolution`, `ResolveError`), engine methods `resolve_conflict`, `resolve_page`, `repair_markers`, `take_over_pull_merge`, `set_block_locator`, `set_journal_template`; `PendingMerge` gained `memo` and `external`; `ConflictRecord` gained `field`, `deleted_by`, `suggestion`, commit/author/time, `resolution`.
- xtask/src/deps.rs: allowed edge bitacora-sync -> bitacora-config (config is a leaf crate; forward edge). Cargo.lock updated (serde, serde_json, bitacora-config for sync; proptest dev).

## Why
Closes the open items of the sync engine record: EDN-aware config merge, device/date whiteboard copies, delete-vs-modify default "restore", prefer by newest writer, persisted merge-state.json plus rerere memo, R14 `id::` writes, R8 external markers and unmerged pulls.

## Verification
`cargo fmt`, `cargo clippy --workspace --all-targets --locked -D warnings` clean; `cargo test -p bitacora-sync -p bitacora-merge -p bitacora-config -p bitacora-core --locked`: sync 75 unit + 8 auto_commit + 17 backends + 18 sync_engine + 18 merge_matrix (new) + 6 onboarding + 4 git2_push + 1 askpass; merge 75 unit incl. 9 new in resolve.rs/page.rs; all pass. `cargo xtask check-deps`, `cargo deny check`, `cargo machete` OK. `typos` reports pre-existing words only (`rela_path`, `ot`).

## Open
- Lazy creation of today's journal (R18, bitacora-core) not done; `locate_block` and journal template need wiring to index/config; whole-tree scan for previously committed markers; visual resolver is BIT-US-0054.
