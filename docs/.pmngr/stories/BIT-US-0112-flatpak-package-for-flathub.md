---
id: BIT-US-0112
type: story
title: Flatpak package for Flathub
status: backlog
priority: low
parent: BIT-EP-0014
milestone: BIT-M-0005
author: mcp
labels: [release, packaging, linux, bitacora-app]
estimate: 5
created: 2026-10-06T14:32:37Z
updated: 2026-10-06T14:32:37Z
---

## Description
As a Linux user on an immutable or Flatpak-first distribution, I want to install Bitacora from Flathub, so that I get sandboxed installs and updates through my software center.

## Acceptance Criteria
- A Flatpak manifest builds offline with generated cargo sources and runs on GNOME and KDE (Wayland and X11).
- Graph folders are reachable via the file-chooser portal (and/or documented `--filesystem` permission), links open via the OpenURI portal, git (CLI, ADR-007) and SSH agent work inside the sandbox, and the MCP port is reachable from host clients.
- A `flatpak` CI job builds the bundle on tags; Flathub submission prepared (metainfo passes `appstream-util validate`).
- Auto-update inside Flatpak is disabled (Flatpak handles updates).

## Notes
- [[crate-stack]] §5.2 (`flatpak` job, later), §4.1 (`rfd` portal support to check); [[gpui-and-gpui-kit]] §1.8 (Zed on Flathub proves GPUI works sandboxed; needs portals), Risk R4.
- ADR-007 (git CLI must be available in the sandbox).
