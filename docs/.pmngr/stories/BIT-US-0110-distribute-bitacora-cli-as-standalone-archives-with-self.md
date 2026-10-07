---
id: BIT-US-0110
type: story
title: Distribute bitacora-cli as standalone archives with self-update
status: done
priority: medium
parent: BIT-EP-0014
milestone: BIT-M-0005
author: mcp
labels: [release, bitacora-cli]
estimate: 3
created: 2026-10-06T14:31:53Z
updated: 2026-10-07T08:21:31Z
started: 2026-10-06T19:59:49Z
closed: 2026-10-07T08:21:31Z
---

## Description
As a user running Bitacora headless (a server, a CI box, an agent sandbox), I want to download `bitacora-cli` as a single binary for my platform and keep it updated, so that I can run `bitacora-cli serve --graph <path>` without the desktop app or GUI libraries.

## Acceptance Criteria
- Each release attaches `bitacora-cli-<version>-<target>.tar.gz|zip` for `x86_64/aarch64-unknown-linux-gnu` (and musl if feasible), `aarch64/x86_64-apple-darwin` (signed + notarized), `x86_64-pc-windows-msvc` (signed), plus shell/PowerShell install scripts.
- `bitacora-cli self-update` replaces the binary from GitHub Releases (verifying the checksum), and refuses when installed via a package manager or the desktop bundle.
- The Linux CLI binary has no runtime dependency on GPU/X11/Wayland libraries (checked with `ldd` in CI).

## Notes
- [[crate-stack]] §4.1 (`self_update 1.3.0`: fine for `bitacora-cli`), §4.2 (MCP must run headless in `bitacora-cli`), Recommendations 5; [[gpui-and-gpui-kit]] §1.8 (`cargo-dist` for CLI-oriented distribution).
- [[mcp-server]] §2 (single instance owns the MCP port; see the single-instance story in this epic).
