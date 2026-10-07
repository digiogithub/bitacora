# Design tokens

Vendored from the Bitacora design system (`/www/Bitacora/bitacora-design-system`, tokens v0.1,
copied 2026-10-07). `tokens/tokens.json` is the source of truth for colours, fonts, radii,
spacing and sizes.

- `tokens/tokens.json` - W3C-style design tokens (edit here, then regenerate).
- `tools/gen_tokens.py`, `tools/check_contrast.py` - the upstream Python tools, kept for
  provenance only. They are NOT part of the build; their paths assume the upstream layout
  (they write `tokens.css` and `rust/palette.rs`).

The Rust port lives in `xtask/src/tokens.rs`:

```bash
cargo xtask tokens            # regenerate crates/bitacora-app/src/ui/theme/palette.rs
cargo xtask tokens --check    # fail if the generated file is stale (CI)
cargo xtask tokens --contrast # WCAG 4.5:1 check of text tokens on every surface (CI)
```

Never edit `palette.rs` by hand.
