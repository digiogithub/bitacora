---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - ui
    - bitacora-app
    - frameless
---
# Frameless main window and AppTitleBar (BIT-US-0120, BIT-US-0121)

Continues [[bitacora-v2-plan]] (D7 / ADR-033) and [[frameless-window]].

## What changed
- New `views::title_bar`: `AppTitleBar` (52px, `left/center/right` slots, drag area, double-click, Linux
  right-click window menu, min/max/close from the granted decorations and `window_controls()`, macOS 78px
  inset), `controls_for`, `left_inset`, `main_window_options` (Client decorations, transparent titlebar,
  `app_owns_titlebar_drag`, title kept, min size). Colours via `ui::theme::Palette` in one `palette()` seam.
- `Workspace::render` puts `AppTitleBar` first and wraps everything in `window_border()` (shadow ring, resize
  bands, tiling insets). `app.rs` builds window options through `main_window_options`; close dispatches `Quit`.
- `ui::frameless` also re-exports `MouseButton`, `WindowControlArea`, `InteractiveElementExt`.
- Removed the CSD spike (`spike/csd.rs`, `--spike-csd`).
- Docs: ADR-033 in `docs/architecture.md`, `docs/design/frameless-checklist.md`, spike outcome note.

## Verification
`cargo clippy -p bitacora-app --all-targets --locked -- -D warnings` clean; `cargo test -p bitacora-app --locked`:
414 passed. macOS/Windows behaviour is implemented from source reading only; manual checklist pending.
