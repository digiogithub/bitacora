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
