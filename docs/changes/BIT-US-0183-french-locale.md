# BIT-US-0183: French UI locale

Part of [[bit-m-0011-owner-improvements-plan]]. Simplified Chinese is delivered separately.

## What changed
- `crates/bitacora-app/src/i18n.rs`: `LANGUAGES` now lists `("fr", "Français")` (order en, es, fr). `match_tag` already reduces any tag by its primary subtag, so `fr_FR.UTF-8`, `fr-CA`, `fr_BE`, `FR` resolve to `fr`; new unit test `french_tags_are_matched`.
- New locale files in `crates/bitacora-app/assets/locales/`: `fr.yml`, `ai.fr.yml`, `editor.fr.yml`, `pando_status.fr.yml`, `settings.fr.yml`, `widgets.fr.yml` (own translations; product names Bitacora, Pando, Logseq, Git, MCP are untouched; every key and `%{placeholder}` mirrors `en.yml`).
- `lib.rs` test `every_key_exists_in_all_locales_with_the_same_placeholders` was already data-driven over `LANGUAGES`, so fr (and zh later) is covered; added `french_translations_resolve_through_rust_i18n`.
- `tests/language.rs`: French case (refusal text, translated slash-menu titles).
- `typos.toml`: fr locale files excluded like the es ones.
- `docs/analysis/rust/crate-stack.md`: i18n row lists the supported languages.

## Verified facts
- The Settings language selector (`views/settings/sections.rs::language_row`) iterates `LANGUAGES` with "Follow the system" as the `None` default; nothing hard-coded.
- `sys-locale 0.3.2` on Linux checks `LANGUAGE`, then `LC_ALL`, `LC_MESSAGES`, `LANG` (POSIX to BCP 47) and returns the first, so LC_ALL > LC_MESSAGES > LANG holds, with the GNU `LANGUAGE` list taking precedence.
- UI dates come from translated `calendar.months` / `calendar.weekdays` (Monday first, `L M M J V S D`); no other locale-dependent date formatting exists.

## Verification
`cargo fmt`, `cargo clippy -p bitacora-app --all-targets --locked -D warnings`, `cargo test -p bitacora-app --locked` (see the branch report for counts).
