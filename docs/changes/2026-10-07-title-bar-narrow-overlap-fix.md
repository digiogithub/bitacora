---
created_at: 2026-10-07T16:00:00Z
updated_at: 2026-10-07T16:00:00Z
tags:
    - change
    - fix
    - app
---
# Fix: title-bar search field overlapped other controls in narrow windows

Continues [[bitacora-v2-plan]]; story BIT-US-0128, task BIT-T-0492.

## What changed
- `views/title_bar.rs` `AppTitleBar`: left/centre/right slots are now separate containers that clip (`overflow_hidden`); the centre slot has a configurable minimum (`center_min_width`) and the left slot yields first. Debug selectors `title-left`, `title-center`, `title-right`, `window-controls`.
- `views/workspace/top_bar.rs` `Workspace::title_bar(breakpoint, cx)`: Medium shrinks the search (flex, 150px min, 300px max, label truncates); Narrow collapses it to an icon button (still opens the palette) and drops the theme and PDF buttons; the tab strip clips.
- `views/workspace.rs`: render passes the `Breakpoint` to `title_bar`.

## Why
A fixed-width search in a flex centre slot spilled over tabs and right buttons when the slot shrank to 0.

## Verification
`#[gpui::test] title_bar_slots_never_overlap_at_any_width` at 1400/1000/760/640/480 asserts slot bounds do not overlap, search stays inside the centre slot, controls stay on screen, theme button hidden at Narrow. `cargo test -p bitacora-app`: 593 passed; clippy clean. No visual check (headless host).
