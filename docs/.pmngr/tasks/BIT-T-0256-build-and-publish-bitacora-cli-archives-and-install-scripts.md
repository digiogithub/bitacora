---
id: BIT-T-0256
type: task
title: Build and publish bitacora-cli archives and install scripts
status: in_review
priority: medium
parent: BIT-US-0110
milestone: BIT-M-0005
author: mcp
labels: [release, ci, bitacora-cli]
estimate: 3
created: 2026-10-06T14:32:06Z
updated: 2026-10-06T19:59:49Z
started: 2026-10-06T19:59:49Z
---

## Description
Evaluate `cargo-dist` (0.32) for `bitacora-cli` only; if it fits alongside the custom release workflow, generate its config (`dist-workspace.toml` limited to `bitacora-cli`), otherwise add a `cli-archives` matrix job to `release.yml` that cross-builds the targets listed in the story (`cross` or native runners), strips, signs (macOS codesign + notarize zip, Windows Authenticode via the bundle signing helpers), archives with LICENSE and README, and uploads to the draft release with checksums. Provide `install.sh` / `install.ps1` that download the right archive, verify `SHA256SUMS` and install to `~/.local/bin` / `%LOCALAPPDATA%\Programs\bitacora`. Add a CI check: `ldd bitacora-cli` on Linux shows no `libvulkan`, `libwayland`, `libxkbcommon`, `libX11`.

## Acceptance Criteria
- Draft release contains all CLI archives; install scripts work on fresh Ubuntu, macOS and Windows runners (CI smoke step: install then `bitacora-cli --version`).

## Notes
- [[gpui-and-gpui-kit]] §1.8 (cargo-dist); [[crate-stack]] §4.2 (headless MCP).
