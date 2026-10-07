---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - design-system
    - app
---
# No-magic-UI-values test and token pipeline docs (BIT-US-0118)

Continues [[bitacora-v2-plan]], [[component-kit]]. New page: [[design-tokens]].

## What changed
- `crates/bitacora-app/src/lib.rs`: tests `views_have_no_magic_ui_values` (BIT-SP-0008.R1: raw colour and `px(<literal>)` size literals in `src/views/**`) and `ok_colour_is_only_used_for_dots_and_icons` (R2). Helpers `view_production_lines`, `calls`.
- New `views/dims.rs`: the single allowlist (named `PX_*` sizes, `SCRIM`, `HIGHLIGHT_BG`, `SYNTAX_KEYWORD`).
- ~200 `px(N)` literals across `views/**` replaced with `dims::PX_N`; `modal.rs` scrim and `block_view.rs` highlight/keyword colours moved to `dims`.
- `chat/render.rs`: edit-applied/answered text no longer uses the `ok` colour (R2), now `text`/`muted`.
- Docs: `docs/design/design-tokens.md` (pipeline tokens.json -> xtask -> palette/scale/bitacora.json -> CI gates -> BitacoraTheme), `design/README.md`, `component-kit.md` links.

## Why
Prevent drift from the generated token theme; no visual change except the two text tints above.

## Verification
`cargo clippy -p bitacora-app --all-targets --locked -- -D warnings` clean; `cargo test -p bitacora-app --locked`: 591 passed lib + 1 doc.
