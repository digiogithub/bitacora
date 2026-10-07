---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - ui
    - editor
---
# Outline block and journal styling (BIT-US-0124)

Continues [[bitacora-v2-plan]], [[component-kit]]. Implements BIT-SP-0008.R1 for BIT-T-0399 and BIT-T-0400.

## What changed
- `views/block_view.rs`: `render_block_row` takes a `&BitacoraTheme` (new param after `theme`). Block text uses `type_scale.body`; headings h1-h3 use `h1_block`..`h3_block` (`heading_style`); bullets use palette `bullet` with `metrics.bullet` / `bullet_child` (depth > 0), folded ring kept; per-ancestor guide lines (`guide_line`, palette `line`); editing rows get `edit_bg`; markers drawn with kit `TaskMarker`, `mutes_block` (via `marker_spec`) mutes the title (`marker_mutes`); priority drawn as kit `Chip`; checkbox restyled (accent / `on_accent`); `#tags` get an accent-tinted background in `style_for`.
- Geometry (indent 24px, gutters) is unchanged so DnD hit-testing (`editor/dnd.rs`) is untouched. Amber is not used (reserved for AI).
- `views/journals.rs`: journal title in `type_scale.display` (Literata) with a `Today` accent chip (`journals.today_badge` i18n key en/es); column width `reading_max` (760) plus side padding.
- `views/page_view.rs`: page title in `type_scale.page_title`; column width from `reading_max`.
- Callers updated: `widgets/query_block.rs`, `widgets/embed_block.rs`.
- No change to parse/serialize or editing semantics; user files untouched.

## Verification
`cargo clippy -p bitacora-app --all-targets -- -D warnings`, `cargo test -p bitacora-app` (see story comment). Screenshot review against `mockups/Main.dc.html` / `Claro.dc.html` was not possible headless; left for manual review.
