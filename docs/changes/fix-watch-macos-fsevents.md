---
created_at: 2026-10-06T21:35:53.989511081Z
updated_at: 2026-10-06T22:11:00.801457853Z
---
# Fix: bitacora-watch on macOS FSEvents

Tags: change, watch, macos

CI job test-core (macos-latest) failed two tests in crates/bitacora-watch/tests/watch.rs: `burst_is_coalesced` (assert at :202, first event hash was not the final content) and `new_directory_with_files_is_scanned` (timeout at :63, no event for pages/sub/g.md).

## Analysis (from notify 8.2.0 and notify-debouncer-full 0.7.0 source)
- Root is already canonicalised in `GraphWatcher::start`; FSEvents reports canonical paths, so path mapping was not the cause.
- notify uses FSEvents by default on macOS (kqueue only with the `macos_kqueue` feature); flags FileEvents|NoDefer, latency 0. No config change needed.
- FSEvents `MustScanSubDirs` becomes `Event(Other, Flag::Rescan)` with the directory path. The debouncer keeps a single rescan event per batch. `handle()` only emitted `WatchEvent::Rescan` and discarded the path, so a new directory coalesced into such a hint produced no file events. Product gap (the runtime did a full reconcile on Rescan, but the watcher itself lost the files).
- `burst_is_coalesced` assumed the first event is the final state; pieces delivered across debounce windows make an intermediate state legitimate.

## Changes
- crates/bitacora-watch/src/watcher.rs: extracted pure `plan(events) -> (rescan, ops)`; a rescan event now also yields `Op::Touch(path)` (walks directories) while still emitting `WatchEvent::Rescan`. Unit tests added.
- crates/bitacora-watch/tests/watch.rs: `burst_is_coalesced` waits for the v2 event, then still asserts no further events.
- crates/bitacora-watch/src/lib.rs: macOS platform notes.

## Verification
Linux: cargo test -p bitacora-watch (8 unit + 12 integration pass), clippy -D warnings clean, typos clean. macOS not runnable here.
Expected on macOS CI: both tests pass; if `new_directory_with_files_is_scanned` still fails, the OS delivered no event at all (not a hint), and a periodic safety reconcile would be the next step.

## Round 2 (new_directory_with_files_is_scanned still failing on macOS)
CI log (run 37537698905) only shows "timed out waiting for event" at tests/watch.rs:63 (16.96 s, i.e. one 15 s wait; cannot tell whether the create or the remove phase). Code review: every Create/Modify/Rename/Remove event path already goes through `Processor::touch`, which walks directories (and `rename` touches both sides when the target is a directory) and, for a vanished directory, emits Removed for all known files below it. So the handler is not ignoring directory events; the likely cause is an event FSEvents never delivered.
Changes (commit 6fe837b):
- `WatchConfig::safety_scan_interval: Option<Duration>` (default 30 s on macOS, `None` elsewhere): a second `Poller` thread (mtime+len rescan, deduped by content hash in `Processor.known`) runs next to the OS watcher. `Shared::spawn_poller` factored out of `start_polling`; `GraphWatcher._safety_stop` owns the stop sender.
- `WatchConfig::trace_events` prints every raw debounced event (`[watch raw] ...`) to stderr.
- tests/watch.rs: fixture enables tracing, prints every emitted WatchEvent (`[watch out]`), and uses a 2 s safety scan so a lost OS event cannot hang a test; libtest shows this output only on failure. New test `safety_scan_reports_new_directory_files` (nested dir create + remove).
- lib.rs docs mention the safety scan.
Verification (Linux): `cargo test -p bitacora-watch --locked` run 4x, all green (8 unit + 13 integration); clippy -D warnings (watch + runtime) clean; typos clean. macOS unverified; if it still fails, the CI log now contains the raw and emitted events.
