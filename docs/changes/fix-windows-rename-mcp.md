---
created_at: 2026-10-06T22:31:49.72341661Z
updated_at: 2026-10-06T22:31:49.72341661Z
tags:
    - change
    - core
    - runtime
    - windows
---
# Fix: Windows failure of mcp_write rename/undo test

CI (windows-latest) failed crates/bitacora-runtime/tests/mcp_write.rs:709: undo_agent_entry of a rename_page returned Err(Changed) (a touched page's fingerprint differed from the loaded snapshot at undo time). Only the panic site is in the log and no Windows host was available, so the root cause is inferred; two real hazards producing this symptom were fixed (commit f262c28):

1. atomic_write / move_file (crates/bitacora-core/src/editor/fsio.rs): on Windows rename over a file another process holds open (antivirus, indexer) fails with access denied, which rename_unsupported treated as "FS cannot rename" and fell back to a NON-atomic in-place write; a watcher reading mid-write sees partial bytes (not an echo) that get merged into the loaded page. New rename_replacing retries with backoff (5 ms doubling, 8 tries, Windows only) before the fallback.
2. Session::external_upsert (crates/bitacora-runtime/src/session.rs): applied bytes captured in the watch event, possibly older than our latest write. It now re-reads the file at apply time (event bytes only if unreadable), so a stale event yields Unchanged instead of rolling a loaded page back.

Verification (Linux): mcp_write 3x green; full cargo test --workspace --locked --no-fail-fast no failures; clippy -D warnings and typos clean. Windows unverified; if it still fails, trace Session::watch_event to find which event touched the page.

See [[changes/fix-watch-macos-fsevents.md]].

## Round 2 (commit 574a3ab on worktree branch)

Windows CI still failed intermittently at mcp_write.rs:709 (undo_agent_entry -> Changed). The fingerprint was already content-based (page_hash of preamble + block ids/parents/depth/text, not mtime), so hypothesis (c) is ruled out. Replaying Windows-style sequences on Linux (Remove(old) + Upserted/Renamed(new) with identical bytes, Remove+Create of the destination and of Linker, stale pre-rename bytes, early destination bytes before the flush) did NOT reproduce the failure before the fix: Pump and core already tolerated them. The one proven remaining hazard is a race: the pump read the file, then queued ExternalChange; a flush of ours between the read and the apply made the page roll back to the older bytes (ids/text change -> fingerprint mismatch). Changes:

- core queue.rs: new Request::ExternalReload { key, fallback }: the worker re-reads the page's file from its store right before apply_external (fallback bytes only if unreadable). Flushes and applies are serialized, so a stale event can no longer roll a page back. Pump::apply (runtime session.rs) uses it; external_upsert no longer reads the file itself.
- core editor/external.rs apply_external: bytes identical to the page's own serialization (clean or dirty, not conflicted) are a true no-op (Unchanged), so block ids never churn on an identical echo.
- runtime store.rs EchoStore: explicit recycle/unrecycle that register the removed source path / restored bytes in the echo filter and keep the inner store's atomic rename (the default copy+remove generated extra events).
- runtime live.rs: #[doc(hidden)] Session::inject_watch_event for tests.
- Tests: core external_changes.rs (identical_bytes_are_a_no_op_even_for_a_dirty_page, external_reload_reads_the_store_so_stale_event_bytes_cannot_roll_a_page_back: the old ExternalChange with stale bytes rolls back); runtime mcp_write.rs windows_style_rename_events_do_not_invalidate_the_agent_undo with a Trace that prints injected watcher events and runtime/queue events on panic (stderr "---- watcher/queue trace ----"), so a Windows CI failure shows which event touched which page.

Verification (Linux): fmt, clippy -D warnings, cargo test --workspace --locked --no-fail-fast no failures, typos clean. Windows unverified; the Linux replay test passes with and without the fix (honest note), the core race test is the fail-before one.
