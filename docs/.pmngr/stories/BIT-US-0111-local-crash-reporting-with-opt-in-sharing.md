---
id: BIT-US-0111
type: story
title: Local crash reporting with opt-in sharing
status: done
priority: medium
parent: BIT-EP-0014
milestone: BIT-M-0005
author: mcp
labels: [release, observability, bitacora-app, bitacora-cli]
estimate: 3
created: 2026-10-06T14:32:14Z
updated: 2026-10-06T20:18:19Z
closed: 2026-10-06T20:18:19Z
---

## Description
As a user hit by a crash, I want Bitacora to save a crash report locally and offer to share it on the next start, so that bugs can be fixed without any silent telemetry and without leaking my notes.

## Acceptance Criteria
- A panic in any thread (UI, tokio, background) writes `<data_dir>/crashes/<timestamp>.json` with version, OS, backtrace, thread name and the last N log lines — never note content, graph paths are redacted to the graph name.
- Native crashes (segfaults in GPU drivers) are captured at least as a "previous session ended abnormally" marker (via the instance sidecar) with the log tail.
- On next launch a dialog offers: View report, Copy to clipboard, Open GitHub issue (prefilled, user reviews before sending), Dismiss. Nothing is uploaded automatically.
- `bitacora-cli` writes the same report format and prints its path.

## Notes
- [[crate-stack]] §5.1 (`panic = "unwind"` kept for crash reporting), §4.1 (`tracing-appender` rolling logs).
- AGENTS.md §3 rule 1 (user data). Epic scope: crash reporting.
