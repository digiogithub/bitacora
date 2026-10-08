# BIT-US-0180: agent model selector

Continues [[2-0-x-owner-feedback-plan]]. No Pando change: everything is client side (generated config plus existing generic endpoints).

## What
- `pando-rs`: `PandoClient::list_models()` for `GET /api/v1/models` with typed `ModelList` / `ModelInfo` (unknown fields ignored). Tests in `tests/kb_client.rs` (mock axum server).
- `bitacora-config`: `PandoSettings.enabled_models: Vec<String>` and `chat_model: Option<String>` (serde default, old `pando.json` files still load); `set_model_enabled`.
- `bitacora-pando`:
  - `managed/config.rs`: `ConfigInput.chat_models`; for each model one extra `[AGUI.Profiles.bitacora-chat--<slug>]` with the same Persona, Prompt, Tools and `Mesnada` as `bitacora-chat` plus `Model = "<id>"`. Duplicate slugs and blanks are skipped.
  - `agents/chat.rs`: `chat_profile_for_model` (slug = TOML bare key), `ModelChoices::from_info` (reads the AG-UI `/info` agents) and `profile_for` (selector choice to profile; unknown choice falls back to `bitacora-chat`).
  - `Supervisor::set_chat_models`, stored by `ManagedSupervisor` and used when the config is written; `service.rs` passes `settings.enabled_models`.
- `bitacora-runtime`: `Session::pando_rest_client`, `ai::{PandoClient, ModelList, ModelInfo}` re-exports.
- Settings > Pando > "Agent models" (`views/settings/pando.rs`): Refresh button, loading / error states, one switch per model. Changing it saves and reopens the graph (the managed config is written at instance start). In external mode the switches are disabled and a hint explains that only the `bitacora-chat--*` profiles the server exposes in `/info` are selectable.
- Agent panel header (`views/chat`): the model chip becomes a dropdown (default entry shows the model `/info` reports for `bitacora-chat`, plus every `bitacora-chat--*` profile the server exposes) when there is anything to choose; the next run posts to that profile. The choice is saved in `pando.json` (`chat_model`) through `ChatViewEvent::ModelChosen`. Writer mode keeps its own profile (no model variants).

## Verification
`cargo test -p bitacora-app -p bitacora-pando -p pando-rs -p bitacora-config -p bitacora-runtime --locked` all pass; clippy `-D warnings` clean on those crates. New tests: `list_models_*`, `model_settings_are_backward_compatible_and_roundtrip`, `enabled_models_get_their_own_chat_profile`, `selector_maps_the_choice_to_a_profile`, `profile_names_are_toml_bare_keys`, `model_rows_drop_auto_and_duplicates_and_sort`. Not verified: a real Pando run with a `Model` profile, and the visuals of the dropdown.

## Follow-up 2026-10-08 (owner validation)
- Settings > Pando > Agent models: filter input on top (`PandoInputs.models_filter`, case-insensitive substring over id, name, provider), "N enabled" counter, list capped at 320px and scrollable (`flex_none` + `max_h`). Pure `filter_models` orders enabled models first, then provider and name; unit test `model_filter_matches_id_name_provider_and_puts_enabled_first`. Strings `settings.pando.models_filter_placeholder`, `models_enabled_count`, `models_no_match` (en, es).
- Agent panel selector: the chip shows only the model name (`model_chip_label`, ellipsis after 22 chars). Dropdown rows come from pure `model_menu_rows`: model name, truncated; selected row has `accent_bg` and a leading dot; the default model gets a muted "default" tag (`chat.model_default_tag`, replaces `chat.model_default`); a model equal to the default is listed once; no checkbox glyphs. Tests in `views/chat/tests.rs`.
- Verification: `cargo test -p bitacora-app --locked` (640 passed), clippy `-D warnings` clean. Not verified visually.
