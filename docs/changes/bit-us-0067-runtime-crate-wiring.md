---
created_at: 2026-10-06T19:08:57.938450697Z
updated_at: 2026-10-06T19:08:57.938450697Z
tags:
    - change
    - runtime
    - watch
    - sync
---
# bitacora-runtime: headless session wiring (ADR-024, BIT-T-0341, BIT-US-0067)

Continues [[bitacora-full-development-plan]], [[changes/bit-us-0067-graph-file-watcher-echo-suppression.md]], [[changes/bit-us-0063-0066-write-pipeline.md]], [[changes/bit-us-0044-0045-auto-commit-and-sync-engine.md]], [[changes/bit-us-0017-0019-mcp-read-tools-resources-prompts.md]], [[changes/bit-us-0052-0053-file-merge-policies-and-conflict-state.md]].

## What changed
- New crate `crates/bitacora-runtime`: `Session::open(RuntimeConfig)` (index open + reconcile, core queue, watcher, optional sync engine, optional MCP), `shutdown(budget) -> ShutdownReport`, `sync_once`, `sync_now`, `open_page`, `subscribe -> RuntimeEvent`.
- `EchoStore` (store.rs): FileStore decorator registering (path, hash) with the watcher EchoFilter BEFORE the atomic rename. No core pre-write hook was needed.
- Pump thread (session.rs): QueueEvent Flushed/FilesApplied -> `Indexer::update_path/delete`; watcher File events -> index + `Request::LoadPage`/`CheckMissing` for loaded pages; `Rescan` -> full reconcile + reload of clean loaded pages; a dirty page is never overwritten (conflict surfaces at flush).
- `QueueGraphWriter` (writer.rs): sync `GraphWriter` over `QueueLock` (Busy/Stale mapping).
- glue.rs: index-backed `locate_block` (R14) and journal template text (children of the block with `template:: <name>` from `:default-templates {:journals}`), MCP `SyncStatusProvider` from the engine handle. Engine uses `JsonMergeStore` when the graph has a git dir.
- MCP uses `bitacora_mcp::IndexGraphReader` and `McpServer::start_with_sync`.
- core: `LoadPage` of an already loaded page now emits `QueueEvent::PageReloaded`.
- cli: `serve` uses the runtime (watcher replaces the 2 s poll; `--sync/--branch/--device`), new `sync --graph` one-shot. xtask deps: runtime edges; app and cli may depend on runtime. ADR-024 in docs/architecture.md; AGENTS.md layout updated. single_writer_guard allowlists cli serve_tests.rs (fixture copy).

## Verification
cargo fmt, clippy --workspace --all-targets --locked -D warnings, check-deps, typos, machete, deny clean. Tests: runtime 8 session + 2 sync (two runtimes through a temp bare repo; collapse + text edit auto-merged, no markers; background auto-commit), cli 8 incl. sync_command and serve e2e; core, mcp, watch, sync suites pass.

## Follow-ups
- App adoption of the runtime (not done here). Background engine has no resolve_conflict command yet (UI story BIT-US-0054); conflict_pages in MCP status is empty.
- Template text rendering is best effort; reference lookup for the id:: union rule is not wired to the index.
- config.edn changes only emit RuntimeEvent::ConfigChanged (no hot reload).
