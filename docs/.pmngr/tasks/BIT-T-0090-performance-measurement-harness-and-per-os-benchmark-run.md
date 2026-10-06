---
id: BIT-T-0090
type: task
title: Performance measurement harness and per-OS benchmark run
status: in_progress
priority: high
parent: BIT-US-0060
milestone: BIT-M-0001
author: mcp
labels: [spike, performance, bitacora-app]
estimate: 3
created: 2026-10-06T14:29:14Z
updated: 2026-10-06T17:33:08Z
started: 2026-10-06T17:33:08Z
---

## Description
Add a debug overlay (toggle action) using `gpui-fps` (or frame timestamps if unavailable for the pinned snapshot) showing FPS and last-frame time; an `--spike-bench` mode that auto-scrolls the 1,000-block page top->bottom->top and types 200 characters into a mid-page block, logging p50/p95 frame times and keystroke-to-paint latency as JSON to the log dir; log RSS (via `sysinfo` or platform API) and time-to-first-frame. Measure release binary size with default GPUI Kit features and with optional tree-sitter features disabled. Run on macOS (Apple Silicon), Windows 11 and Linux (X11 + Wayland; one Intel/AMD iGPU, optionally NVIDIA).

## Acceptance Criteria
- JSON results for each OS committed under `docs/design/spike-results/` (small files) and summarised in the spike report.
- Pass/fail against the targets stated in the story.

## Notes
- [[gpui-and-gpui-kit]] §1.5, §1.10 (binary size), Risk R4, R8, open question on binary size/cold start; [[crate-stack]] §4.1.
