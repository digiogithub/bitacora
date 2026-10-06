---
id: BIT-T-0033
type: task
title: "CI: build and upload per-OS app artifacts and smoke-test the window"
status: backlog
priority: high
parent: BIT-US-0014
milestone: BIT-M-0001
author: mcp
labels: [ci, bitacora-app]
estimate: 2
created: 2026-10-06T14:27:12Z
updated: 2026-10-06T14:27:12Z
---

## Description
Add a `build-app` job (matrix ubuntu-24.04, macos-latest, windows-latest; runs on `main` and on PRs labelled `build-artifacts`) that runs `cargo build -p bitacora-app --profile release` and uploads `bitacora` / `bitacora.exe` (zipped, with the Linux binary noting its Vulkan requirement) via `actions/upload-artifact` (retention 14 days). Add `bitacora --smoke-test` flag: opens the window, waits for the first frame, logs "first frame rendered in N ms" and exits 0; run it on the Linux leg under `xvfb-run` (if wgpu/Vulkan via lavapipe works) and on macOS/Windows runners, marking it `continue-on-error` if a runner lacks a GPU.

## Acceptance Criteria
- Artifacts for the 3 OSes are downloadable from a `main` run.
- A manual check list in the PR records the window opening on Linux X11, Linux Wayland, macOS and Windows from those artifacts.
- Smoke-test timing appears in the job log where it can run.

## Notes
- Epic acceptance criterion; [[gpui-and-gpui-kit]] §1.5 (wgpu needs Vulkan on Linux; GL fallback unverified), §1.7.
