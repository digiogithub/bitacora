---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - app
    - design-system
    - components
---
# Design system component kit

Implements BIT-US-0117 (tasks BIT-T-0385, BIT-T-0386, BIT-T-0387) of [[bitacora-v2-plan]]. API
summary in [[design/component-kit.md]]; builds on the `BitacoraTheme` global.

## What changed

- New module `crates/bitacora-app/src/views/kit/`: `Button`, `IconButton`, `Glyph`/`glyph`,
  `Kbd`, `Chip`, `TaskMarker`, `Pill`, `Overline`, `Card`, `Tab`, `Segmented`, `PopoverShell`,
  `Gallery`, plus the pure spec functions (`button_spec`, `icon_button_spec`, `marker_spec`,
  `chip_colors`, `pill_colors`, `tab_colors`, `segment_colors`, `fade_duration`).
- Facade `crates/bitacora-app/src/ui/mod.rs`: new `ui::assets::icon_svg` (embedded Lucide
  catalogue) and re-exports `Animation`, `AnimationExt`, `Div`, `ElementId`, `KeyUpEvent`,
  `Keystroke`.
- `views/mod.rs`: `pub mod kit;`. New doc `docs/design/component-kit.md`.

## Why

Screen stories (top bar, sidebar, outline, right panel, tasks, popovers) share one set of
components styled only from tokens, so a token or mode change reaches every screen.

## Decisions

- Icons: Lucide from the kit's `AllAssets`, stroke rewritten to `metrics.icon_stroke` (1.7);
  the sparkle is a custom filled four-point SVG. No Lucide path data is copied.
- Tab follows `docs/componentes.md` (bordered, overlapping tab), not the backlog's "underline".
- Popover: Escape handled by the shell (it takes focus), outside press via `on_mouse_down_out`;
  the owner removes it. Fade skipped under reduce-motion.
- The gallery text is unlocalised sample copy (developer page), routed through `sample()` to
  satisfy the hard-coded-string lint.

## Verification

`cargo clippy -p bitacora-app --all-targets --locked -- -D warnings` clean;
`cargo test -p bitacora-app --locked`: 437 passed (18 new: icons, specs, gallery view tests in
light and dark: token heights, enabled/disabled click, Enter activation, selection, popover
Escape and outside press).
