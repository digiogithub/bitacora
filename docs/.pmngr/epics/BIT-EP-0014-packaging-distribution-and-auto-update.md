---
id: BIT-EP-0014
type: epic
title: Packaging, distribution and auto-update
status: done
priority: medium
milestone: BIT-M-0005
author: mcp
labels: [infra, release]
created: 2026-10-06T14:21:13Z
updated: 2026-10-07T08:21:44Z
started: 2026-10-07T00:15:15Z
closed: 2026-10-07T08:21:44Z
---

## Description
cargo-packager bundles (dmg signed + notarized, MSI/NSIS signed, deb + AppImage, later Flatpak), GitHub Releases pipeline on tags, Velopack auto-update spike and implementation (fallback: update notice), crash reporting, `bitacora-cli` distribution, single-instance handling (the app must own the MCP port).

## Acceptance Criteria
- Tagged release produces installable artifacts for the 3 OSes.
- Installed app updates itself (or notifies) to a newer release.

## Notes
See [[crate-stack]] §5.2.
