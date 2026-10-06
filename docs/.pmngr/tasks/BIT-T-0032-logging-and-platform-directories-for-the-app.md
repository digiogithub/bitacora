---
id: BIT-T-0032
type: task
title: Logging and platform directories for the app
status: in_progress
priority: high
parent: BIT-US-0014
milestone: BIT-M-0001
author: mcp
labels: [bitacora-app, observability]
estimate: 2
created: 2026-10-06T14:27:12Z
updated: 2026-10-06T16:46:40Z
started: 2026-10-06T16:46:40Z
---

## Description
- `src/paths.rs`: `AppDirs` built from `directories::ProjectDirs::from("es", "Digio", "Bitacora")` exposing `config_dir`, `data_dir` (index dbs live under `<data_dir>/graphs/<graph-hash>/` per ADR-005), `cache_dir`, `log_dir`; create on first use.
- `src/logging.rs`: `tracing-subscriber` with `EnvFilter` (default `info,bitacora=debug` in debug builds), stderr layer + `tracing-appender` daily rolling file in `log_dir` (keep 7 files), non-blocking guard kept alive for the process lifetime.
- Log app version, OS, GPU backend info (if exposed by GPUI) at startup.

## Acceptance Criteria
- Starting the app creates the directories and a log file containing the startup line.
- `RUST_LOG=trace` overrides the default filter.
- Unit test for `AppDirs` using a temp HOME override where supported.

## Notes
- [[crate-stack]] §4.1 (`directories 6`, `tracing`, `tracing-appender`), ADR-005.
