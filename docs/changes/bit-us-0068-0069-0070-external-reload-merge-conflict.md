---
created_at: 2026-10-06T19:43:19.135434113Z
updated_at: 2026-10-06T19:43:19.135434113Z
tags:
    - change
    - core
    - runtime
    - merge
---
# BIT-US-0068 / 0069 / 0070: external reload, 3-way merge, conflict notice (core part)

Continues [[bitacora-full-development-plan]], [[changes/bit-us-0067-runtime-crate-wiring]], [[changes/bit-us-0051-structural-merge-and-page-serialization]]; design [[block-editor]] sections 4 and 6, [[git-sync-merge]], ADR-016/017.

## What changed
- `bitacora-core/src/editor/external.rs` (new): `align`, `diff_blocks`, `Workspace::apply_external(key, bytes) -> ExternalOutcome` (Unchanged / Reloaded / Merged / Conflict / Unknown), `ConflictNotice` (disk snapshot, base_available, `PageConflict`s from `merge_page`, 2-way `BlockDiff`s), `ReloadReport` (kept/removed/added/changed ids), `ExternalEvent`, `set_editing_block`, `conflict`, `take_external_events`.
- Ids stay stable: `Page::load_reusing` (model.rs) takes ids from `bitacora_merge::match_pages` alignment (uuid, LCS, fuzzy). No second matcher.
- Dirty page: `merge_page(base = disk.bytes, ours = page.serialize(), theirs = new bytes)`. Clean result replaces the page (aligned ids), disk snapshot = theirs, page stays dirty and is flushed normally; the merge result is applied as bytes, not as `Vec<Op>` (deviation from the story wording: no ops, not undoable, history untouched). Conflict (or no base / non-UTF-8) keeps ours, parks the page in `Workspace::conflicted`, flush skips it (reported in `conflicts`), no markers ever written.
- `flush.rs`: pre-write `DiskState::Changed` now merges instead of just reporting; `resolve_keep_mine` clears the conflict first; `take_disk` reloads with stable ids.
- `queue.rs`: `Request::ExternalChange{key,bytes}`, `Response::External`, `QueueEvent::{PageMerged, PageConflicted(Arc<ConflictNotice>), EditingBlockChanged}`, `QueueError::PageConflicted`, `CommandQueue::{set_editing_block, conflict, conflicts}`. `LoadPage` of an existing page at the same path/title and sync `apply_edits` reload keep block ids. MCP (`Source::Mcp`) Run/Commit on a conflicted page is rolled back and refused.
- `bitacora-runtime/src/session.rs`: watcher events and rescan go through `ExternalChange` (dirty pages are merged too); new `RuntimeEvent::ExternalMerged`; `ExternalChangeWhileDirty` now means an unmergeable conflict.
- Tests: `bitacora-core/tests/external_changes.rs` (13), `bitacora-runtime/tests/external.rs` (4), updated `editor_ops.rs` and `runtime/tests/session.rs`.

## Notes
- Tiny blocks (fewer than 4 tokens) only match exactly (bitacora-merge matcher rule), so edit-vs-edit of a one-word block merges as delete + add rather than a conflict; no data is lost.
- Editing lock: the UI buffer lives in the app; core reports `EditingBlockChanged{mine, disk}` and the page model takes the disk text. Keep mine / take disk / keep both on commit is app work (BIT-T-0344). Undo truncation for vanished blocks: the app uses `ReloadReport.removed` (surfaced through the outcome; undo stacks live in the app).
- Banner/diff UI (BIT-T-0349) is app work.

## Verification
`cargo test -p bitacora-core -p bitacora-runtime --locked` green; clippy workspace and typos run, see commit.
