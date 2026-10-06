---
id: BIT-T-0010
type: task
title: "CI workflow: lint job and test-app 3-OS matrix"
status: backlog
priority: critical
parent: BIT-US-0003
milestone: BIT-M-0001
author: mcp
labels: [infra, ci, bitacora-app]
estimate: 3
created: 2026-10-06T14:25:20Z
updated: 2026-10-06T14:25:20Z
---

## Description
Extend `.github/workflows/ci.yml`:
- `lint` (macos-latest, as GPUI Kit does): `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- `test-app` matrix: `ubuntu-24.04` (runs `script/install-linux-deps.sh`), `macos-latest` (aarch64), `windows-latest`: `cargo test --locked -p bitacora-app` (GPUI `#[gpui::test]` uses the headless test platform). Keep `fail-fast: false`.
- Optional `macos-13` (x86_64) entry gated behind a workflow input, since Intel runners may be retired.

## Acceptance Criteria
- All matrix legs are green on the skeleton including a placeholder `#[gpui::test]` in `bitacora-app`.
- A clippy warning in any crate fails `lint`.
- Each leg's wall time is recorded in the PR description as the baseline.

## Notes
- [[crate-stack]] §5.2, [[gpui-and-gpui-kit]] §1.7 (macOS `runtime_shaders`, Windows MSVC preinstalled).
