---
created_at: 2026-10-09T08:00:00Z
updated_at: 2026-10-09T08:00:00Z
tags:
    - change
    - ui
    - title-bar
---
# Title-bar tabs collapse into a dropdown button (BIT-US-0182)

Part of [[bit-m-0011-owner-improvements-plan]]; fixes the known issue in [[2026-10-07-title-bar-narrow-overlap-fix]] (many tabs clipped the `+` button) on top of [[2026-10-07-top-bar-content-and-tabs]].

## What changed
- `views/workspace/top_bar.rs`
  - Pure fit logic: `tab_width_estimate`, `tab_budget` (window width minus controls, right slot, centre minimum and the other left controls; named f32 constants) and `tabs_fit(available, widths, gap, plus_w)`. Stateless, re-evaluated on every render, so resize and tab open/close update it and there is no boundary flicker.
  - `Workspace::title_bar(breakpoint, window_w, cx)` now takes the window width; when the tabs do not fit it renders `tab_overflow` instead of the strip. The `+` button stays after it.
  - `tab_overflow`: a kit `Tab` (active look, focusable with the accent ring) showing the active tab's glyph, truncated title, `(count)` and a chevron. It opens a `PopoverShell` dropdown (same `deferred(anchored())` pattern as the Graph menu) with one row per tab (kit `Button`, glyph + title, the active one bordered) and a close `IconButton` per row when more than one tab is open. A row activates its tab and closes the menu; closing a tab keeps the menu open.
- `views/kit/tab.rs`: `Tab::suffix` and `Tab::trailing` (non-truncating text and an end glyph).
- `views/workspace.rs`: `tab_menu_open`, `tabs_collapsed` fields; render passes the window width.
- No new locale strings: the dropdown reuses tab titles and the count is numeric.

## Notes
- Widths are estimates (the bar is flex laid out): a wrong guess collapses early but never overlaps, since each slot clips its own content. At Wide/Medium widths on Linux/Windows with client decorations the budget is tight (about 70px at 900px, 450px at 1280px).

## Verification
Unit tests for the fit/budget functions; gpui tests `crowded_tabs_collapse_into_one_overflow_button`, `overflow_dropdown_lists_activates_and_closes_tabs`; `title_bar_slots_never_overlap_at_any_width` extended with 8 long-titled tabs. `cargo test -p bitacora-app`: 647 passed; clippy clean. Visual check not done (headless).
