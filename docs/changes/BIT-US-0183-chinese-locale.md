# BIT-US-0183: Simplified Chinese UI locale

Part of [[bit-m-0011-owner-improvements-plan]] (the French part of the story is recorded separately).

## What changed

- `crates/bitacora-app/src/i18n.rs`: `LANGUAGES` gains `("zh", "中文")`. `match_tag` compares only the primary subtag, so `zh_CN.UTF-8`, `zh-CN`, `zh-Hans`, `zh-Hans-CN`, `zh_SG` and the Traditional `zh_TW`, `zh-Hant`, `zh_HK` all resolve to `zh`. Only Simplified ships; Traditional users get it as the closest translation (documented on `match_tag`). New test `every_chinese_variant_maps_to_simplified_chinese`.
- `crates/bitacora-app/assets/locales/`: `zh.yml`, `ai.zh.yml`, `editor.zh.yml`, `pando_status.zh.yml`, `settings.zh.yml`, `widgets.zh.yml` (1009 translated keys, same key set and `%{...}` placeholders as `en.yml`; own translations, product names untranslated). The `i18n!("assets/locales", fallback = "en")` macro derives the locale from the file stem, so `zh` matches `resolve()`.
- Parity: `every_key_exists_in_all_locales_with_the_same_placeholders` already iterates `LANGUAGES`, so it covers `zh` without changes. New test `chinese_translations_resolve_through_rust_i18n` in `src/lib.rs`.
- `typos.toml`: zh locale files excluded.
- `crates/bitacora-app/src/fonts.rs`: new test `chinese_text_falls_back_to_a_system_font`.

## CJK rendering findings

The embedded families (Atkinson Hyperlegible, Literata) have no Han glyphs. GPUI falls back automatically, so no fallback list is configured and no CJK font is bundled:

- Linux/wgpu: `gpui-pre-wgpu-0.3.8/src/cosmic_text_system.rs:552-590` (`font_id_for_cosmic_id`) registers the system font cosmic-text picks when the requested face lacks a character. `FontFallbacks` (`:354-366`) is an optional explicit user chain, empty by default.
- macOS (CoreText cascade) and Windows (DirectWrite fallback) substitute system CJK fonts natively; not verifiable on this host.
- Verified on Linux with Noto Sans CJK installed: shaping "设置搜索" with `FONT_UI` yields four glyphs, none `.notdef`. The test returns early when the host has no CJK font.

If a platform ever needs an explicit chain, set `Font::fallbacks` with Noto Sans CJK SC, Noto Sans SC, Source Han Sans SC, PingFang SC, Hiragino Sans GB, Microsoft YaHei, WenQuanYi Micro Hei.

## Verification

`cargo fmt --all`, `cargo clippy -p bitacora-app --all-targets --locked -- -D warnings`, `cargo test -p bitacora-app --locked`.
