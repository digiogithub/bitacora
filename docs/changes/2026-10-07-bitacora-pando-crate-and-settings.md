---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - pando
    - settings
    - runtime
---
# Pando settings, keychain credentials and the bitacora-pando crate (BIT-US-0136, BIT-US-0135)

Implements BIT-T-0426, BIT-T-0427, BIT-T-0423, BIT-T-0424 and BIT-T-0425 (partly, see below) of [[bitacora-v2-plan]] (D2, D3). ADR-028 and ADR-029 rows added to [[architecture]]; design in [[pando-integration]].

## What changed

- `crates/bitacora-config/src/pando.rs`: `PandoSettings`, `PandoMode`, `PandoFeature`, `GraphConsent`, `validate_pando_url`, `PandoUrl`, `UrlRole`, `PandoSettingsError`; atomic `load`/`save`. bitacora-config gained `serde` and `serde_json` dependencies.
- New crate `crates/bitacora-pando`: `credentials.rs` (`PandoCredentials`, `SecretBackend`, `KeyringBackend`, `MemoryBackend`, `TokenKind`, env overrides), `service.rs` (`PandoService`, `PandoOptions`, `Endpoints`), `events.rs` (`PandoStatus`, `PandoEvent`, `EventSink`), `supervisor.rs` (`Supervisor`, `ManagedEndpoint`, the seam for BIT-US-0141).
- `crates/bitacora-runtime`: `RuntimeConfig::pando`, `Session::pando()`, `pando_status()`, `pando_events()`; Pando stopped first in `stop_all`; re-exports of the Pando types. Test `tests/pando.rs`.
- `xtask/src/deps.rs`: allowed edges for `bitacora-pando` and `pando-rs`, new check that `pando-rs`/`bitacora-pando` never enter the `bitacora-core` closure, tests.
- `.github/workflows/ci.yml`: `pando-rs`, `bitacora-pando`, `bitacora-runtime` added to the test-core job and the gpui-kit guard.
- `AGENTS.md` layout and dependency direction; `docs/architecture.md` crate map and ADR-028/029.

## Why

One integration point for UI and CLI, opt-in with consent, machine-local settings and keychain tokens (ADR-028/029). Not done: `bitacora-pando` does not yet depend on `bitacora-core`/`bitacora-index` (nothing uses them until the sync stories; cargo-machete would flag them); ADR-031 is out of scope of these stories; the app does not yet build `PandoOptions` from a settings file (UI story).

## Verification

`cargo clippy -p bitacora-config -p bitacora-pando -p bitacora-runtime -p xtask --all-targets -- -D warnings`, `cargo test` for the same crates, `cargo run -p xtask -- check-deps` (OK, 16 crates), `cargo tree -p bitacora-core` free of tokio and pando.
