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
