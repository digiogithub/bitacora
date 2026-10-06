---
created_at: 2026-10-06T21:20:01.093197826Z
updated_at: 2026-10-06T21:20:01.093197826Z
tags:
    - change
    - sync
---
# Fix: cross-OS sync/watch/app test failures (Windows, macOS)

Source: CI run 37530766816 on main. Continues [[fix-path-canonicalization-macos-windows]].

## Root causes
1. **gix honoured the runner's global `core.autocrlf=true`** (Windows). All failures labelled GixOnly (no system git) came from this: `clone_graph` checked out `- home\r\n` (onboarding.rs:211), clone/merge results had CRLF (merge_matrix.rs:551), and a freshly cloned device saw its work tree as modified and auto-committed, so heads diverged (sync_engine.rs:32 in the setup helper, merge_matrix.rs:232, recovery.rs:146, testkit git.rs:28 push "fetch first"). Real product bug: user graphs on Windows would get CRLF rewrites. Fix in `crates/bitacora-sync/src/backend/gix_net.rs`: all `gix::open` go through `open_gix()` with `config_overrides(["core.autocrlf=false"])`, and `clone_repo` uses `with_in_memory_config_overrides`. The CLI backend already passed `-c core.autocrlf=false`.
2. **git2/WinHTTP error text** for a refused connection ("failed to send request: A connection with the server could not be established") was classified `Other`, not `Network` (git2_push.rs:153). `classify_failure` in `backend/mod.rs` now matches those phrases.
3. **sync_prefs local URL classification**: `/srv/git/notes.git` is not `Path::is_absolute` on Windows. `is_rooted_local_path` in `crates/bitacora-app/src/sync_prefs.rs` accepts leading `/` everywhere and drive/UNC paths on Windows (test extended under cfg(windows)).
4. **separate_gitdir test (macOS and Windows)**: git writes the canonical gitdir (`/private/var`, long Windows name, `/`), test compared against the raw tempdir spelling. Test-only; now compares canonicalized paths (`tests/onboarding.rs`). Product code returns what git recorded, which is correct.
5. **watch own_delete_is_suppressed (Windows)**: the debounced creation event of the setup file arrived after watcher start and was counted as a leak. The test now only asserts the `Removed` event is suppressed (`bitacora-watch/tests/watch.rs`).

## Not diagnosed
- macOS `bitacora-watch` `new_directory_with_files_is_scanned` (watch.rs:63, timed out waiting for event; FSEvents timing) — not touched.
- `markers_committed_by_another_tool...` (testkit git.rs:28 push rejected) is expected to be fixed by cause 1 but cannot be confirmed without Windows.

## Verification
Linux: clippy -D warnings clean; `cargo test --workspace --locked --no-fail-fast` no failures. Windows/macOS unverified locally.
