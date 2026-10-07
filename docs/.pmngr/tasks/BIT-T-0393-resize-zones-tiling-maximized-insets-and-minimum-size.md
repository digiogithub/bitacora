---
id: BIT-T-0393
type: task
title: Resize zones, tiling/maximized insets and minimum size
status: in_review
priority: medium
parent: BIT-US-0121
milestone: BIT-M-0006
author: mcp
labels: [v2, frameless, linux]
estimate: 2
created: 2026-10-07T09:13:09Z
updated: 2026-10-07T10:39:17Z
started: 2026-10-07T10:39:17Z
---

## Description
Verify `Root::decorate`/`window_border()` resize zones and cursors under CSD, set `set_client_inset` for shadow, drop insets/rounded corners when tiled or maximized (`Decorations::Client { tiling }`), enforce a minimum window size suited to the 252+360 layout.

## Acceptance Criteria
- Resize from all 8 handles on GNOME/KDE Wayland; no gap when maximized or tiled.
