# Design token pipeline

How a design decision becomes a pixel in Bitacora (BIT-US-0118, BIT-SP-0008.R1/R2, plan [[bitacora-v2-plan]]).
Component usage rules live in [[component-kit]].

## Pipeline

```
design/tokens/tokens.json            source of truth (W3C-style tokens, vendored from the design system)
        |  cargo xtask tokens        (xtask/src/tokens.rs)
        v
crates/bitacora-app/src/ui/theme/
    palette.rs                       Palette: dark()/light(), entries()     (GENERATED)
    scale.rs                         TypeScale (12 styles) + Metrics         (GENERATED)
crates/bitacora-app/assets/themes/
    bitacora.json                    Bitacora Light/Dark kit themes          (GENERATED)
        |  loaded at startup
        v
BitacoraTheme global                 cx.bitacora(): colors, type_scale, metrics
        |
        v
views/**                             read tokens only; never literals
```

Never edit the generated files by hand. To change a value: edit `tokens.json`, run
`cargo xtask tokens`, commit the regenerated files.

## CI gates

```bash
cargo xtask tokens --check     # fails if the generated files are stale versus tokens.json
cargo xtask tokens --contrast  # WCAG 4.5:1 for every text token on every surface, dark and light
```

Both run in `.github/workflows/ci.yml`. `xtask/src/tokens.rs#vendored_tokens_pass_contrast`
covers the contrast rule locally.

## No magic UI values (BIT-SP-0008.R1)

`crates/bitacora-app/src/lib.rs` has two source-scan tests over `src/views/**` (test modules,
`*_tests.rs` and comments are skipped):

- `views_have_no_magic_ui_values` fails on `rgb(`, `rgba(`, `hsl(`, `hsla(`, `rems(`, `0xRRGGBB`,
  `"#rrggbb"` and on `px(<non-zero number>)`.
- `ok_colour_is_only_used_for_dots_and_icons` (R2) fails when `colors.ok` / `c.ok` appears on a
  line that is not a status dot, glyph/icon or graph tag dot.

The single allowlist is `views/dims.rs`: named constants for design-mockup literals that have no
token yet (`PX_*` sizes, `SCRIM`, `HIGHLIGHT_BG`, `SYNTAX_KEYWORD`). Rules for contributors:

1. Prefer a token: `cx.bitacora().metrics` (`space[n]`, `radius_*`, `icon*`), `type_scale`, `colors`.
2. If the design needs a literal without a token, add a constant to `dims.rs` and use it.
3. Better still, add the token to `tokens.json` and migrate the use site, then drop the constant.
4. Dynamic sizes (`px(width)`, `px(r * 2.0)`) are fine; only literals are flagged.

## Open questions

- Many `dims::PX_*` uses are close to a spacing token (e.g. `PX_4`, `PX_8`, `PX_12` equal
  `space[2]`, `space[4]`, `space[6]`); migrating them to `metrics.space` is a follow-up that needs
  a metrics handle at each use site.
