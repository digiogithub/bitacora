---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - app
    - a11y
---
# Narrow-window behaviour and layout version bump (BIT-US-0128)

Part of [[bitacora-v2-plan]]; story BIT-US-0128 (BIT-SP-0008.R6).

## What changed
- New `crates/bitacora-app/src/views/responsive.rs`: `Breakpoint` (Wide/Medium/Narrow), thresholds 1170 and 760.
- `views/workspace.rs`: `Workspace::render` computes the breakpoint from the viewport; closes the right dock when the window is too narrow for it, hides the inline sidebar below 760px and shows it as an overlay via `ToggleLeftSidebar` (`narrow_sidebar_open`). New test `narrow_windows_close_the_right_panel_and_overlay_the_sidebar`.
- `layout.rs`: `LAYOUT_VERSION` 1 -> 2 so persisted layouts pick up the 360px right dock.
- `docs/design/accessibility-1.0.md` section 5.

## Verification
`cargo test -p bitacora-app --lib --locked` and clippy (see final report).

## Gaps
Right panel is not a true overlay when reopened below 1170px; no automated Tab-order test.
