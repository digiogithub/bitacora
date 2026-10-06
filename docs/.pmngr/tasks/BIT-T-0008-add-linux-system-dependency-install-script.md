---
id: BIT-T-0008
type: task
title: Add Linux system-dependency install script
status: in_review
priority: critical
parent: BIT-US-0003
milestone: BIT-M-0001
author: mcp
labels: [infra, ci, linux]
estimate: 1
created: 2026-10-06T14:25:20Z
updated: 2026-10-06T16:55:34Z
started: 2026-10-06T16:55:34Z
---

## Description
Create `script/install-linux-deps.sh` (bash, `set -euo pipefail`, idempotent, uses `sudo` only when not root) installing the Ubuntu 24.04 package list from [[crate-stack]] §5.2: `gcc g++ clang cmake pkg-config mold libfontconfig-dev libfreetype-dev libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev libx11-xcb-dev libxcb1-dev libvulkan1 mesa-vulkan-drivers libssl-dev libzstd-dev libasound2-dev libdbus-1-dev`. Support a `--minimal` flag that installs only what `test-core` needs (compiler, pkg-config, `libdbus-1-dev` if keyring needs it). Reference the script from AGENTS.md §4 ("Linux build deps").

## Acceptance Criteria
- Running the script on a fresh `ubuntu:24.04` container followed by `cargo build -p bitacora-app` succeeds.
- `--minimal` is enough for `cargo test -p bitacora-core -p bitacora-markdown`.

## Notes
- [[crate-stack]] §5.2, [[gpui-and-gpui-kit]] §1.7. Do not install `libgit2-dev`/`libsqlite3-dev` (bundled/vendored).
