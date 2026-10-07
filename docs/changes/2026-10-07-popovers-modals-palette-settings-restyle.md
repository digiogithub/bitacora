---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - ui
    - design-system
---
# Popovers, modals, palette and settings window restyle (BIT-US-0127)

Part of [[bitacora-v2-plan]]; builds on [[component-kit]].

## What changed
- `views/modal.rs`: `modal`, `title_bar`, `labelled` keep their signatures but render through a
  `Themed` RenderOnce reading `cx.bitacora()`: raised surface, `line` border, `radius_popover`,
  `shadow_lg`, `h3_block` title, 0.4 black scrim (no scrim token exists). Affects all 8 dialogs
  (sync, conflicts, history, disk conflict, credentials, agent activity, settings).
- `views/palette.rs`: palette card uses the popover surface tokens.
- `editor/element.rs` (`completion_popup`, `edit_content`) and `editor/view.rs` (`row_edit`): the
  page-ref / slash / tag completion list and the date picker use the popover surface, `ui_small`
  rows with `accent_bg` selection and `radius_control`.
- `views/settings/mod.rs`: nav rebuilt from tokens (`side` background, `Overline` group labels,
  36px items, kit glyphs). New `Section::Pando` (AI) nav slot with a placeholder pane
  (`render_pando`) and `Section::GROUPS`; BIT-US-0137 fills the content. Locale keys
  `settings.group.*`, `settings.section.pando`, `settings.pando.placeholder` (en, es).

## Verification
`cargo clippy -p bitacora-app --all-targets --locked -D warnings` clean; `cargo test -p bitacora-app`: 482 passed
(plus new `navigation_groups_cover_every_section_once_with_a_pando_slot`).

## Notes
Dialogs are not wrapped in kit `PopoverShell` because it takes focus and handles Escape itself,
which would fight the dialogs' own key handling. Settings rows/fields still use the kit theme.
