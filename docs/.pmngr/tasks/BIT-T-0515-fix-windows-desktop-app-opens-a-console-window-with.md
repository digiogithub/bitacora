---
id: BIT-T-0515
type: task
title: "Fix: Windows desktop app opens a console window with internal logs"
status: in_review
priority: high
parent: BIT-US-0161
milestone: BIT-M-0009
author: mcp
labels: [bitacora-app, windows, bug]
created: 2026-10-14T11:10:04Z
updated: 2026-10-14T11:10:04Z
started: 2026-10-14T11:10:04Z
---

## Description
Owner report: launching `bitacora.exe` on Windows first opens a terminal window showing the app log, then the desktop window; closing the terminal ends the app. Linux/macOS show no terminal. Cause: the binary used the default console subsystem.

## Acceptance Criteria
- No console window on launch from Explorer, Start menu, shortcut or installer (ADR-036).
- Started from a terminal, `--help`, logs and bench output still print there; redirected/piped output (`| tee`, askpass pipes) keeps working.
- No console window flashes for child processes (git, hostname, pando).
- `bitacora-cli.exe` stays a console program.

## Notes
Implemented: `windows_subsystem = "windows"` + `console::attach_parent` in `crates/bitacora-app/src/main.rs`; `bitacora_sync::process::hide_console` and `bitacora-pando` `sys::hide_console` (`CREATE_NO_WINDOW`). Manual Windows check pending (host is Linux).
