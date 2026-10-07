---
created_at: 2026-10-07T12:00:00.000000000Z
updated_at: 2026-10-07T12:00:00.000000000Z
tags:
    - change
    - theme
    - design-system
---
# BitacoraTheme global and design-token kit themes

Continues [[bitacora-v2-plan]] (BIT-US-0115, BIT-US-0116; BIT-SP-0008.R1) and ADR-032.

## What changed
- `cargo xtask tokens` now also generates `crates/bitacora-app/src/ui/theme/scale.rs` (`TypeStyle`, `TypeScale`, `Metrics`, from `typography`, `size`, `radius`, `space` tokens plus a small `EXTRA_METRICS` table for values only in the design `theme.rs`) and `crates/bitacora-app/assets/themes/bitacora.json` ("Bitacora Light" / "Bitacora Dark", from `KIT_MAP` in `xtask/src/tokens.rs`, following design-system `docs/gpui-kit.md`, plus `font.family` / `mono_font.family`). `--check` covers all three files.
- New `ui/theme/bitacora.rs`: `BitacoraTheme` global (`mode`, `colors: Palette`, `type_scale`, `metrics`), `Mode`, `BitacoraTheme::print()` (always light), `ActiveBitacoraTheme::bitacora()` on `App`, `TypeStyleExt::type_style`. Re-exported from `crate::ui::theme`.
- `theme.rs`: `apply` keeps the global in sync with the resolved mode; the light/dark kit slots always point at the Bitacora themes.
- Removed: Paper, Solarized, Midnight, user theme files (`reload_user_themes`, `user_theme_errors`, Settings "Your themes" row), per-mode theme selection (`set_theme_name`, theme lists in the Theme menu), `AppSettings::light_theme/dark_theme` (old keys are ignored on load and dropped on next save, so users fall back to the Bitacora theme for their mode). `custom.css` overrides unchanged.
- ADR-032 row and `docs/design/accessibility-1.0.md` updated.

## Notes
- Kit base font size is left at the kit default (16): the design's 14 px UI size is carried by `TypeScale::ui` and views adopt it in later stories.
- Screenshot review in both modes was not possible in the headless Linux agent environment; left for manual validation.

## Verification
`cargo test -p bitacora-app -p xtask --locked` (413 + 21 pass), `cargo clippy -p bitacora-app -p xtask --all-targets --locked -- -D warnings`, `cargo xtask tokens --check` and `--contrast`. New tests in `theme.rs`: global follows mode, every type style/metric resolves, kit colours equal palette values, kit JSON keys exist in the kit schema, 1.x theme names fall back.
