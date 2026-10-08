# BIT-US-0176: git over HTTPS asks for credentials (multi-call askpass)

Part of [[2-0-x-owner-feedback-plan]]. Design background: [[git-sync-merge]].

## Why
Releases ship only the `bitacora` executable, so `askpass_helper_path()` never found
`bitacora-askpass`, `GIT_ASKPASS` stayed empty and git failed silently.

## What changed
- `bitacora-sync/src/askpass.rs`: `ENV_MODE` (`BITACORA_ASKPASS_MODE=1`), `mode_requested`,
  `run_if_askpass_mode()`; `AskpassServer::env()` now also sets the marker.
- `bitacora-app/src/main.rs`: `main` calls `run_if_askpass_mode()` first and exits before logging,
  GPUI or single-instance init.
- `bitacora-app/src/credentials.rs`: `askpass_helper_path()` falls back to `current_exe()`;
  `CredentialHub::forget_remote`.
- `bitacora-sync/src/credentials.rs`: `ChainProvider::forget_remote` (URL, scheme+host origin and
  key passphrase entries in the keychain, clears sticky cancels).
- Settings > Sync: "Forget saved credentials" row (`SettingsEvent::ForgetCredentials`), en + es.
- Auth failures carry `HTTPS_AUTH_HINT`; `SyncError::Auth` message now shows the hint.
- `bitacora-sync/tests/askpass_helper.rs`: runs the real helper binary against a loopback server.

## Verified
- Credentials only go through `KeyringStore`; the helper receives only the loopback addr/token via env.
- git2 push fallback already uses the same `CredentialProvider` chain.
- `bitacora-cli` has no credential wiring (no askpass); not changed.
- clippy (-D warnings) on bitacora-sync and bitacora-app; bitacora-sync tests; bitacora-app lib tests (626 passed).
