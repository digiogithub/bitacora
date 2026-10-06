---
id: BIT-T-0155
type: task
title: Keep running in background when the last window closes
status: backlog
priority: medium
parent: BIT-US-0086
milestone: BIT-M-0005
author: mcp
labels: [release, bitacora-app]
estimate: 3
created: 2026-10-06T14:30:22Z
updated: 2026-10-06T14:30:22Z
---

## Description
Add the setting `app.keep_running_in_background` (default on). When on, closing the last window keeps the process, tokio runtime and MCP server alive; re-launching (via the single-instance IPC) or clicking the dock icon reopens the window. Provide a tray/menu-bar icon with Open, MCP status, Quit if the pinned GPUI snapshot or a license-compatible crate (`tray-icon`, MIT/Apache) supports it on the 3 OSes; otherwise document the limitation and rely on the dock (macOS) / relaunch (Windows/Linux). Optional "start at login" via `auto-launch` crate, starting hidden.

## Acceptance Criteria
- With the setting on, an MCP request succeeds after the window is closed; Quit (menu or tray) stops the server and releases the lock.
- With the setting off, closing the last window quits.
- Manual verification recorded for the 3 OSes.

## Notes
- [[mcp-server]] §2 ("always running", tray mode, start at login) and open question 4 (writes confirmation without a window). Check tray support in GPUI before adding crates (ADR-014 license check).
