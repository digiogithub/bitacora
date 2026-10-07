---
id: BIT-T-0050
type: task
title: Theme switching with system appearance and persisted choice
status: done
priority: medium
parent: BIT-US-0025
milestone: BIT-M-0001
author: mcp
labels: [ui, theming, bitacora-app]
estimate: 2
created: 2026-10-06T14:28:06Z
updated: 2026-10-07T08:21:12Z
started: 2026-10-06T16:46:40Z
closed: 2026-10-07T08:21:12Z
---

## Description
- `src/theme.rs`: initialise GPUI Kit `Theme`/`ThemeRegistry`; mode `System | Light | Dark` (System follows `window.appearance()` and observes changes); selectable bundled theme names.
- Persist `{mode, light_theme, dark_theme}` in a minimal `AppSettings` struct (`serde`, JSON at `<config_dir>/settings.json`) — the full settings UI is out of scope (BIT-EP-0013).
- Menu entries / actions `ToggleTheme` and "Select theme..." (simple list in a GPUI Kit Popover or Menu).

## Acceptance Criteria
- Switching the OS appearance with mode=System updates the window without restart (manual check on macOS and one Linux DE).
- Selected theme persists across restarts; unknown theme name falls back to default with a warning.

## Notes
- [[gpui-and-gpui-kit]] §1.3 (Theming), §2.2 (Theme). Do not reuse Zed's GPL `theme` crate (ADR-014).
