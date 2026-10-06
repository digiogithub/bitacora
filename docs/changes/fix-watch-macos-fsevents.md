---
created_at: 2026-10-06T22:11:00.802387991Z
updated_at: 2026-10-06T22:31:54.220333711Z
tags:
    - change
    - watch
    - macos
---
# Fix: bitacora-watch on macOS FSEvents

(Rounds 1 and 2 unchanged; see git history of this page. Summary: round 1 made rescan hints touch their path (watcher.rs `plan`) and relaxed `burst_is_coalesced`; round 2 (6fe837b) added `WatchConfig::safety_scan_interval` (poller next to the OS watcher, 30 s on macOS) and `trace_events` raw tracing, plus test `safety_scan_reports_new_directory_files`.)

## Round 3 (removal phase of new_directory_with_files_is_scanned)
CI log: FSEvents delivered Create(Folder)/Create(File) but NO raw event for remove_dir_all. The safety Poller could not compensate: its mtime baseline never contained pages/sub/g.md (created and deleted between two 2 s ticks), so its diff saw nothing. Fix (commit f262c28): Processor::sweep_missing() (crates/bitacora-watch/src/process.rs) stats every file in `known` (already announced) and reports Removed for those gone; poll::spawn (poll.rs) calls it after each tick. Covers any lost removal, also in the polling fallback. Test stays strict. Linux: cargo test -p bitacora-watch --locked 3x green. Expectation on macOS: Removed within one safety interval (2 s in tests, 30 s in production).
See [[changes/fix-windows-rename-mcp.md]].

## Round 4 (root cause: the test fixture never applied the 2 s safety interval on macOS)
Round 3's sweep logic was correct, but `start_with` in crates/bitacora-watch/tests/watch.rs used `cfg.safety_scan_interval.or(Some(2 s))`. `WatchConfig::default()` is already `Some(30 s)` on macOS (None elsewhere), so `.or` kept 30 s there: the poller ran, but its first tick after the removal came ~30 s later, beyond the 15 s WAIT. On Linux the default is None, so tests always got 2 s and never exposed it. Fix: the fixture now caps any requested interval at 2 s (`map_or(2 s, |i| i.min(2 s))`).
Also: `WatchConfig::drop_os_events` (doc-hidden test switch; handle() discards all OS events) plus Linux test `safety_scan_alone_reports_create_and_remove_when_os_is_silent` (creation and removal reported only via poller + sweep_missing; passes). Diagnostics with `trace_events`: `[watch poll] started, interval ..`, `[watch poll] tick: N changed path(s) [..]`, `[watch poll] sweep: N missing of M known`; `sweep_missing` now returns the count. Verified: clippy -D warnings clean, `cargo test -p bitacora-watch --locked` 3x green (14 integration tests).
