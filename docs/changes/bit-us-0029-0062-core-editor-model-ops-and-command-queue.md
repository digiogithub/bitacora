---
created_at: 2026-10-06T18:39:41.307926846Z
updated_at: 2026-10-06T18:39:41.307926846Z
tags:
    - change
    - core
    - editor
---
# BIT-US-0029 + BIT-US-0062: core editing model, Ops, transactions and command queue

Continues [[bitacora-full-development-plan]]; designs: [[block-editor]], [[04-editor-outliner-operations]], [[bit-us-0092-bit-us-0093-serializer-and-surgical-edits]], [[bit-us-0044-0045-auto-commit-and-sync-engine]], [[bit-us-0006-0007-index-writer-and-reconcile]].

## What changed (crates/bitacora-core)
- `src/editor/model.rs`: `BlockId` (session-stable u64, `IdGen`), `Block`, `Origin` (page, byte generation, RawBlock, depth, text hash), `Subtree`, `Position`, `Page` (tree over a Document with cleared nodes; `serialize()` emits clean blocks verbatim, edited via canonical writer, re-parses output and falls back to full canonical render on mismatch; `mark_saved()` rebases origins).
- `src/editor/workspace.rs`: `Workspace` (mutable graph; named differently from read-only `graph::Graph`), global block index, uuid count index with per-transaction delta, attach/detach/reparent primitives.
- `src/editor/op.rs`: `Op` = InsertSubtree, RemoveSubtree, Move, SetText, EditText, AdoptChildren, SetPreamble, CreatePage, DeletePage, RenameFile with `apply` (fills captured data) and `inverse`.
- `src/editor/tx.rs`: `Transaction`, `Workspace::commit` (rollback of applied prefix, invariants: acyclic, single parent slot, consistent index, no new duplicate uuid), `run(cmd)`, `undo(tx)` building block (history stack = BIT-US-0039).
- `src/editor/cmd.rs`: `Cmd`/`plan`/`Refusal` skeleton: SetText, InsertSibling/Child, DeleteBlocks, Indent (expands parent), Outdent (direct mode), MoveBlocks, SetCollapsed, SetProperty.
- `src/editor/flush.rs`: `FileStore` trait, `FsStore` (temp + fsync + rename), `MemStore`, `Workspace::flush` with pre-write content check (a changed file is reported as conflict, never overwritten).
- `src/queue.rs`: `CommandQueue` single consumer thread owning Workspace + FileStore; `Source` Ui/Mcp/Sync/External; oneshot `Reply` (blocking and Future); audit ring; `PageSnapshot` published after each change; `QueueEvent` observers (Committed, Flushed, FilesApplied, PageReloaded) for the indexer/echo filter wiring in app/cli; `acquire`/`QueueLock::apply(FileEdit)` with expected-content check for the sync adapter.
- Tests: tests/editor_ops.rs (17), editor_props.rs (proptests + every fixture page round-trips through the model), command_queue.rs (10 incl. 2x1000 stress), single_writer_guard.rs (source scan; clippy disallowed-methods rejected because it would hit tests in every crate).

## Wiring notes
- Sync adapter (in app/cli): `GraphWriter::acquire` -> `CommandQueue::acquire(timeout)` (maps Busy), `GraphLock::apply(&[FileChange])` -> `QueueLock::apply(Vec<FileEdit>)` (1:1 mapping; Stale -> WriterError::Stale).
- Indexer: register an observer in `QueueConfig`; on `Flushed`/`FilesApplied` call the IndexWriter with written paths; hashes in `WrittenFile` feed echo suppression.
- Later stories: debounce (US-0063) drives `Workspace::flush`; self-check (US-0064) already inside `Page::serialize`; hash merge (US-0065) replaces the "conflict" outcome; History (US-0039).

## Verification
`cargo fmt`, `cargo clippy -p bitacora-core --all-targets --locked -D warnings` clean; `cargo test -p bitacora-core --locked`: 44 unit + 17 + 3 + 10 + 5 + 2 integration tests pass; `cargo xtask check-deps` OK; `cargo deny check` OK.

## Known limits
- A block text containing a line that re-parses as a bullet/heading (e.g. "x\n- y") is not representable; no invariant rejects it yet (property tests exclude `-`, `#`).
- Snapshots are rebuilt per touched page after each commit (O(page)); structural sharing is a later optimization.
