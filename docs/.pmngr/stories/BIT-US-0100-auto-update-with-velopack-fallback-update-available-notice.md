---
id: BIT-US-0100
type: story
title: "Auto-update with Velopack (fallback: update-available notice)"
status: backlog
priority: medium
parent: BIT-EP-0014
milestone: BIT-M-0005
author: mcp
labels: [release, auto-update, bitacora-app]
estimate: 8
created: 2026-10-06T14:31:22Z
updated: 2026-10-06T14:31:22Z
---

## Description
As a user, I want Bitacora to tell me about new releases and update itself safely, so that I get fixes without reinstalling — and without the updater ever touching my graph or interrupting unsaved edits.

## Acceptance Criteria
- A spike decides Velopack vs notice-only per OS (compatibility with notarized `.app`, MSI/NSIS installs, AppImage/deb) and records the decision as an ADR row in [[architecture]] (closes the open decision on the auto-update channel).
- The installed app checks GitHub Releases (stable or beta channel setting) at most once per day and on demand ("Check for updates…").
- With Velopack: the update downloads in the background (delta when available) and applies on restart only after pending writes are flushed and git sync is idle.
- Fallback: a non-blocking notification with release notes link and download button.
- Update checks can be disabled in settings; no telemetry is sent beyond the GitHub API request.

## Notes
- [[crate-stack]] §4.1 (`velopack 1.2.161` evaluate; `self_update` wrong for GUI; `cargo-packager-updater` alternative), Risk R7, Recommendations 5, open question on Velopack + notarization/MSIX.
- [[architecture]] §7 open decision "Auto-update channel (Velopack vs notice + download)". AGENTS.md §3 rule 1, 3.
