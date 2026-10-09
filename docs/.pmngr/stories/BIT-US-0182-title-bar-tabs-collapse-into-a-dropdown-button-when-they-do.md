---
id: BIT-US-0182
type: story
title: Title-bar tabs collapse into a dropdown button when they do not fit
status: backlog
priority: high
parent: BIT-EP-0027
milestone: BIT-M-0011
author: mcp
labels: [bitacora-app, ui, title-bar, dock]
estimate: 3
created: 2026-10-09T07:10:42Z
updated: 2026-10-09T07:10:42Z
---

## Description
Today many tabs clip the `+` button (known issue in `changes/2026-10-07-title-bar-narrow-overlap-fix.md`). When the page tabs (`Workspace::title_bar` in `crates/bitacora-app/src/views/workspace/top_bar.rs`) do not fit in the left slot, render a single overflow button at the first tab's position instead of the strip. The button shows the active tab's glyph + title (truncated) and a count/chevron; clicking it opens a dropdown menu anchored under it listing every tab (glyph + title, active one marked, click activates, a close affordance per row when more than one tab). The `+` new-tab button stays visible after it.

Fit decision is a pure function (e.g. `tabs_fit(available_px, tab_widths, plus_width) -> bool`) using the tab min/max widths from the design metrics; available width comes from the window width minus the other slots (same budget logic as the existing narrow/overlap handling). Re-evaluated on window resize and on tab open/close.

## Acceptance Criteria
- With few tabs: unchanged strip. With many tabs or narrow window: one overflow button + `+`; nothing overlaps (extend `title_bar_slots_never_overlap_at_any_width`).
- Dropdown activates/closes tabs; keyboard reachable with focus ring.
- Unit tests for the fit function and a gpui test for the collapsed state; new strings in `en.yml`/`es.yml`.
