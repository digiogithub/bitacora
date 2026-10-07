---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
---
# Design tokens pipeline and generated palette

Implements BIT-US-0113 (BIT-T-0377, BIT-T-0378) of [[bitacora-v2-plan]] (decision D6, ADR-032).

## What changed
- `design/tokens/tokens.json` vendored from the design system; upstream `gen_tokens.py` / `check_contrast.py` kept in `design/tools/` for provenance; `design/README.md` documents origin and commands.
- `xtask/src/tokens.rs` (`cargo xtask tokens [--check|--contrast]`): Rust port of the generator and contrast check. Generates `crates/bitacora-app/src/ui/theme/palette.rs` (`Palette`, `dark()`, `light()`, `entries()`).
- `crates/bitacora-app/src/ui/mod.rs`: `ui::theme::palette` / `ui::theme::Palette`, `ui::{rgb, rgba}` facade re-exports; test module `ui/theme/palette_tests.rs` asserts both palettes equal the JSON.
- Workspace `serde_json` gets `preserve_order` (token order drives the generated field order).
- `.github/workflows/ci.yml`: `cargo xtask tokens --check` and `--contrast` in the lint job. ADR-032 row added to `docs/architecture.md`.
- The existing theme (`src/theme.rs`) is untouched; no visual switch yet (BIT-US-0115/0116).

## Verification
- `cargo xtask tokens --check` up to date; `--contrast` 64/64 OK.
- `cargo test -p xtask` 21 passed; `cargo test -p bitacora-app --lib` 401 passed (incl. palette equality and `only_ui_names_gpui_kit`); clippy `-D warnings` clean; `cargo deny check` ok.
- Note: this host lacks the `libxkbcommon-x11.so` dev symlink, so app tests were linked with an extra `-L` dir containing it.
