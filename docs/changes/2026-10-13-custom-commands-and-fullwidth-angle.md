---
created_at: 2026-10-13T10:30:00Z
updated_at: 2026-10-13T10:30:00Z
tags:
    - change
    - editor
    - config
---
# Custom `:commands` and the full-width `<` trigger (BIT-T-0512, BIT-T-0513)

Part of [[angle-block-commands-gaps-plan]] (story BIT-US-0188); continues [[2026-10-07-special-blocks]].

## What changed

- **BIT-T-0512.** `:commands` of `logseq/config.edn` (`[["js" "Javascript"]]`) are listed after the built-ins in
  the `<` and `/` menus. Accepting one inserts the text verbatim in place of trigger + query, caret at the end.
  Vector/step values and malformed entries are skipped (counted, one `tracing::warn!`), never a config error.
- **BIT-T-0513.** Typing `〈` right after `《` is replaced by `<` and the `<` menu opens (Logseq
  `handler/editor.cljs:3010-3018`, read only). The word-start rule is unchanged and documented as an intentional
  divergence: `a<b` does not open the menu.

## Files and symbols

- `crates/bitacora-config/src/accessors.rs`: `CustomCommand`, `CustomCommands`, `EffectiveConfig::custom_commands`
  (exported from `lib.rs`); the comment-preserving writer is untouched.
- `crates/bitacora-app/src/editor/commands.rs`: `filter_custom`, `apply_custom`.
- `crates/bitacora-app/src/editor/completion.rs`: `Item::Custom`, `command_items`, `candidates(.., custom)`,
  `fullwidth_angle`.
- `crates/bitacora-app/src/editor/view.rs`: `custom_commands` field filled in `set_config`; `accept_completion`
  handles `Item::Custom`; `replace_text_in_range` applies the full-width swap.
- `crates/bitacora-app/src/editor/view/slash.rs`: `run_custom_command`.
- `docs/design/block-editor.md` section 7.4.

## Why

Logseq users keep custom snippets in `:commands`; the full-width pair is how Chinese/Japanese IMEs type `<`.

## Verification

- `cargo test -p bitacora-config --locked` (new `custom_commands_*` tests in `tests/load.rs`).
- `cargo test -p bitacora-app --locked --lib editor::` (191 passed), including the `#[gpui::test]`s
  `custom_config_commands_join_both_menus_and_insert_verbatim` and
  `full_width_angle_pair_becomes_an_angle_trigger_but_a_mid_word_less_than_does_not`, and unit tests in
  `commands.rs` / `completion.rs`.
- `cargo clippy -p bitacora-config -p bitacora-app --all-targets --locked -- -D warnings`.

## Notes

Custom entries are always listed after the built-ins (not merged by score). The config is read when the editor
receives `set_config`, i.e. when the page view is rebuilt after a config reload.
