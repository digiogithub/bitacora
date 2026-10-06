---
id: BIT-T-0269
type: task
title: Panic hook and crash report writer shared by app and CLI
status: done
priority: medium
parent: BIT-US-0111
milestone: BIT-M-0005
author: mcp
labels: [observability, bitacora-core, bitacora-app, bitacora-cli]
estimate: 2
created: 2026-10-06T14:32:30Z
updated: 2026-10-06T20:18:19Z
closed: 2026-10-06T20:18:19Z
---

## Description
Add `bitacora_core::diagnostics::crash` (sync, no UI deps): `install_panic_hook(CrashConfig { data_dir, app_name, version, log_dir })` that chains the previous hook and writes an atomic JSON report (`version`, `git_sha` from build env, `os`, `arch`, `thread`, `message`, `location`, `backtrace` via `std::backtrace::Backtrace::force_capture()`, `log_tail` = last 200 lines of the current log file, `session_id`). Redact absolute paths under the user's home and graph roots (`~/…`, `<graph>/…`). Mark the session as "clean exit" in the instance sidecar on normal shutdown so the next start can detect abnormal termination (native crash). Install the hook first thing in `bitacora-app` and `bitacora-cli` `main`. Keep at most 20 reports (delete oldest).

## Acceptance Criteria
- Test: a panic in a spawned thread produces a report with the thread name and a redacted path.
- Test: the abnormal-termination marker is set when the process is killed (`SIGKILL` on Unix, `TerminateProcess` on Windows) and cleared on clean exit.
- No block/page content appears in reports (test with a panic message containing a fixture path).

## Notes
- [[crate-stack]] §5.1 (`panic = "unwind"`), §4.1 (`tracing-appender`). ADR-012 (core stays sync). AGENTS.md §3 rule 4 (atomic writes).
