---
created_at: 2026-10-06T19:13:54.441092855Z
updated_at: 2026-10-06T19:13:54.441092855Z
tags:
    - change
    - bitacora-core
---
# BIT-US-0028 / 0057 / 0089 / 0099: lazy pages, today's journal, recycle, new graph

Part of [[bitacora-full-development-plan]]; builds on [[changes/bit-us-0029-0062-core-editor-model-ops-and-command-queue.md]] and [[changes/bit-us-0063-0066-write-pipeline.md]]. Specs: [[01-file-graph-layout]], BIT-SP-0002 (R1, R4, R7, R11, R12, R14, R16, R18, R19). Commit 2e5f654.

## What changed (crate bitacora-core)
- `editor/model.rs`: `Page.lazy: LazyCreation {auto_preamble, pristine}`, `Page::is_blank`, `is_virtual` (never written and blank/pristine); `needs_write` ignores virtual pages (so no empty files, ever).
- `editor/lifecycle.rs` (new): `Workspace::open_page`, `ensure_journal`, `ensure_today`, `find_template` (+ `<% today/yesterday/tomorrow/current page %>` expansion), `new_page_path`, `auto_title_preamble`, `DayRollover`, `Opened`, `LifecycleError`. Opening a virtual page is not a transaction.
- `date.rs`: `to_unix_days`, `from_unix_days`, `add_days`, `from_unix_secs`, `secs_until_midnight` (midnight timer helper for the app).
- `editor/cmd.rs`: `Cmd::SetPageProperty` (pre-block `k:: v`, or `k: v` in front matter), `Cmd::DeletePage`, `Cmd::DeleteAsset`, `Refusal::NotAnAsset`.
- `recycle.rs` (new): `recycle_path`, `asset_path_from_link`, `asset_links`.
- `editor/flush.rs`/`fsio.rs`: `FileStore::recycle`/`unrecycle` (FsStore renames, overwrites); pending deletes now recycle instead of unlink; `FlushReport.recycled`; pending restores. `editor/op.rs`: `Op::DeleteAsset`/`RestoreAsset`. `queue.rs`: schedules pending restores like deletes.
- `new_graph.rs` (new): `create_graph` (pages/contents.md `-`, journals/, logseq/config.edn from `DEFAULT_CONFIG_EDN`, empty custom.css, logseq/.recycle/); refuses non-empty folders (ignores `.git`/`.gitignore`), no-op for an existing graph.
- `tests/single_writer_guard.rs`: allow-list entry for `new_graph.rs`.
- Tests: `tests/page_lifecycle.rs` (27).

## Verification
`cargo test -p bitacora-core -p bitacora-config --locked`: all pass (core 59 unit + integration incl. 27 new). `cargo clippy --workspace --all-targets --locked -- -D warnings` clean, typos clean. Note: `single_writer_guard` currently reports violations in bitacora-cli (`serve.rs`, `serve_tests.rs`), pre-existing from another agent, not from this change.

## Open points / decisions
- App wiring left: T-0084 (call `ensure_today` on startup and from a midnight timer using `DayRollover`), T-0174 UI confirmation dialog, favorites removal call-site (`ConfigEditor::favorites_remove`).
- Undo of a flushed page delete rewrites the file from the model (recycle copy remains); asset undo moves the recycled copy back.
- Template block with children: children only unless `template-including-parent:: true`.
- Adding a page property to a front-matter page makes the canonical writer insert one blank line after the closing `---`.
