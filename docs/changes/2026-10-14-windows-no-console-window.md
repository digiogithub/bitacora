---
created_at: 2026-10-14T11:10:20.807507615Z
updated_at: 2026-10-14T11:10:34.094109733Z
tags:
    - change
    - fix
    - windows
---
# Windows: no console window for the desktop app (BIT-T-0515)

Records ADR-036 in [[architecture]]; part of the v2 per-OS pass (BIT-US-0161).

## What changed

- `crates/bitacora-app/src/main.rs`: `#![cfg_attr(windows, windows_subsystem = "windows")]`, so launching
  `bitacora.exe` no longer opens a terminal with the log (closing it used to end the app). New Windows-only
  `console::attach_parent` (kernel32 `AttachConsole`/`GetStdHandle`/`SetStdHandle`, no new dependency): when started
  from a terminal and stdout/stderr are unset, attaches to the parent console (opening `CONOUT$` if needed) so
  `--help`, logs, `--smoke-test` and bench output still print. Redirected or piped handles (CI `| tee`, git askpass
  pipes) are left alone; Explorer/Start-menu launches stay silent. Runs after the askpass early exit.
- `crates/bitacora-sync/src/process.rs` (new): `hide_console` sets `CREATE_NO_WINDOW` on Windows; used by
  `backend/cli.rs` `run_with_env` (every git CLI call), `backend/detect.rs` `probe`, `engine.rs` (rebase abort) and
  `repo_setup.rs` (`hostname`). Without it a GUI process would flash a console per git call.
- `crates/bitacora-pando/src/managed/sys.rs`: `hide_console`, applied by the non-unix `prepare` and by the
  `--version` probe in `supervise.rs`.
- `bitacora-cli` unchanged (console program). Logs still go to the rolling file in the cache dir.

## Verification

Linux: `cargo clippy --workspace --all-targets --locked -D warnings`; `cargo test -p bitacora-sync -p bitacora-pando
-p bitacora-app` all pass. Windows: the full crate graph cannot be cross-checked on the Linux host (C deps need a
mingw toolchain), so the new Windows-only code was compiled and clippy-checked (`-D warnings`, `unsafe_code = warn`)
in an isolated crate for `x86_64-pc-windows-gnullvm`. CI Windows smoke steps pipe output, so they still wait for the
process and capture its log. Manual check on Windows pending (BIT-T-0515 in review). Commit 7787644.
