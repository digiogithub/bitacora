---
created_at: 2026-10-07T00:00:00Z
updated_at: 2026-10-07T00:00:00Z
tags:
    - change
    - fonts
    - design-system
---
# Embedded design-system fonts (BIT-US-0114)

Continues [[bitacora-v2-plan]]; implements BIT-SP-0008.R3 (stories BIT-US-0114, tasks BIT-T-0379, BIT-T-0380).

## What changed
- `crates/bitacora-app/assets/fonts/`: 8 static TTFs (Atkinson Hyperlegible Next Regular/Medium/SemiBold/Bold, Atkinson Hyperlegible Mono Regular/Medium, Literata Medium/SemiBold) plus `OFL-AtkinsonHyperlegible.txt` and `OFL-Literata.txt`.
- `src/fonts.rs` (new): `FONT_UI`, `FONT_DISPLAY`, `FONT_MONO`, `register(cx)` (`include_bytes!` + `text_system().add_fonts`, failure logged not fatal). Called in `app::start` right after `ui::init`, before any window.
- `src/ui/mod.rs`: re-exports `TextSystem`; `testing::platform_text_system()` (real platform font stack for tests).
- Licensing: `NOTICE` font section, packager `resources` ship the OFL texts in bundles under `licenses/`, `deny.toml` comment (fonts are data, not crates; `cargo deny check licenses` ok).
- Not changed: default theme font family (US-0113 owns the palette/tokens); user `font-family` / `font-size` overrides keep working (existing `theme.rs` tests pass).

## Notes
Literata static TTFs carry internal names "Literata 36pt <Weight>" but resolve under family `Literata`.

## Verification
`cargo test -p bitacora-app --lib` 400 passed (incl. `fonts::tests::embedded_families_resolve`: families present in the text system and every embedded weight resolves); clippy `-D warnings` clean; `cargo deny check licenses` ok. Linking on this machine needed `LIBRARY_PATH` pointing at a `libxkbcommon-x11.so` symlink (missing -dev package, environment only).
