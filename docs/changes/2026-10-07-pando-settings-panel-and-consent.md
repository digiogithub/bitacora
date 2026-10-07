---
created_at: 2026-10-07T15:00:00Z
updated_at: 2026-10-07T15:00:00Z
tags:
    - change
    - ui
    - pando
    - privacy
---
# Pando settings panel, per-graph consent and exclusions (BIT-US-0137, BIT-US-0138)

Part of [[bitacora-v2-plan]]; implements BIT-SP-0009.R2/R6 and BIT-SP-0010.R2, see [[pando-integration]].

## What changed
- `bitacora-app` `views/settings/pando.rs` (new): the `Section::Pando` page. Status chip, master
  switch, mode (managed / external / off), REST and AG-UI URLs, allow remote, tokens (masked inputs
  that store into the OS keychain and are cleared; only the source "keychain / env var / none" is
  shown), Test connection (version, minimum-version warning), managed instance state with
  Restart / Open log, a note on whether the managed instance shares the user's KB, feature
  switches, and per graph: consent (dialog), agent writes switch, exclusions editor, sync counts.
- Settings are saved to `pando.json` (`bitacora_config::PandoSettings::save`, atomic) and nothing
  else; `SettingsEvent::PandoChanged { reopen }` tells the workspace. Enable, mode, endpoints,
  tokens, features, consent grant and agent writes reopen the graph; exclusions and revoked consent
  apply to the running session (`Session::apply_pando_consent`) without reopening.
- `session.rs`: `SessionOptions::pando_settings_path`; `runtime_config` builds `PandoOptions` with
  `pando_options_from_file` (a broken file is logged and ignored). `app.rs` passes
  `default_pando_settings_path()`.
- `workspace.rs`: `WorkspaceConfig::pando_settings_path`, handles `PandoChanged`, and calls
  `RightPanel::set_agent_configured` (`settings::agent_configured_for`) on session start and after
  each change: Pando active, chat feature on and the graph consented.
- `bitacora-runtime` `Session::apply_pando_consent(&PandoSettings, purge)`: sets the semantic
  `ContentPolicy` (revoked consent gives `ContentPolicy::denying_all()`; documents already sent are
  deleted by the reconcile), refreshes the `pando` token's `ReadExclusions`, optionally purges.
  Re-exports the pando connection and credential types the page needs.
- `bitacora-pando`: `connection` module (`test_connection`, `kb_sharing`, version check);
  `ContentPolicy::deny_all` / `denying_all()`; `ManagedState` exported.
- Locales: `settings.pando.*` in English and Spanish.

## Why
Pando is opt-in and privacy-sensitive: the consent text says the indexed blocks go into Pando's
shared KB (agent memory) readable by any Pando agent, that `private::` pages and exclusions are never
sent, and that agent writes are a separate opt-in. The same exclusions feed the semantic
`ContentPolicy` and the MCP `ReadExclusions`.

## Limits and decisions
- `GET /health` gives only the version and startup mode, so "Test connection" cannot show agents or
  the model (the user story mentions `/info`). Per-feature profile pickers are not exposed: the
  managed config defines the profiles and `PandoSettings::profiles` stays editable in `pando.json`.
- The feature list is the three `PandoFeature`s (semantic search, chat, MCP bridge); journal review,
  recommendations and inline AI have no switch yet.
- The shared-KB note reads `<pando config dir>/.pando.toml` for an absolute `[Data] Directory`
  (design section 5.4).
- Revoke applies at once to the running session; the purge needs that live worker, so "remove my
  data" is a revoke-time option rather than a later action.

## Verification
- `cargo test -p bitacora-app` (518 pass): `views::settings::tests::pando_ui::*` (graph folder
  byte-identical after saving all Pando settings, validation refuses bad URLs without writing,
  tokens only in the keychain, consent dialog text and records, exclusions add/remove/dedupe,
  connection test against a local HTTP server, shared KB note, every mode renders), `session` test
  for `pando_options_from_file` wiring.
- `cargo test -p bitacora-runtime --test pando_semantic` (exclusions and revoked consent reach a
  running session and delete documents), `cargo test -p bitacora-pando` (`connection`, `denying_all`).
- `cargo clippy -p bitacora-pando -p bitacora-runtime -p bitacora-app --all-targets --locked -- -D warnings` clean.
