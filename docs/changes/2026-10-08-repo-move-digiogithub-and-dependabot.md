---
created_at: 2026-10-08T07:54:25.436534804Z
updated_at: 2026-10-08T07:54:25.436534804Z
tags:
    - changes
    - release
    - security
    - dependencies
---
# Repo move to digiogithub/bitacora + dependabot merges (2026-10-08)

## Repository URL fix (commit 6835bda, fmt follow-up 2339abb)
Repo moved from github.com/sevir/bitacora to github.com/digiogithub/bitacora. Code previously pointed at `digio-es/bitacora`, an account the project never owned (supply-chain risk: Velopack updater and `bitacora-cli self-update` would install releases from it; SHA256SUMS comes from the same release so it gives no protection).

Replaced `digio-es/bitacora` -> `digiogithub/bitacora` in:
- `Cargo.toml` (`repository`, feeds `CARGO_PKG_REPOSITORY` used by `bitacora-app` update::service::REPOSITORY and crash issue URL)
- `crates/bitacora-app/Cargo.toml` (`homepage`)
- `crates/bitacora-cli/src/cmd/self_update.rs` (`DEFAULT_API`)
- `crates/bitacora-runtime/src/crash.rs`, `crates/bitacora-app/src/update/release.rs` (tests)
- `xtask/src/release.rs` (attestation verify hint)
- `packaging/install/install.sh`, `packaging/install/install.ps1`, `packaging/flatpak/es.digio.bitacora.metainfo.xml`

Follow-ups: v2.0.0/v2.0.1 binaries still have `digio-es` baked in; the `digio-es` GitHub name should be registered by the owner to block hijack. Consider release signing independent of the repo (minisign-style key in binary).

## Dependabot merges
- getrandom 0.3 -> 0.4 (bitacora-mcp, bitacora-runtime): compiles, tests pass.
- sha2 0.10 -> 0.11 (bitacora-cli, xtask): tests pass, `cargo xtask fixtures verify` OK (429 files, hex output unchanged).
- GitHub Actions group: checkout v7, upload-artifact v6, download-artifact v7, setup-rust-toolchain v2 (build-warnings=deny replaces RUSTFLAGS -D warnings), attest-build-provenance v4, azure/trusted-signing-action v2.0.0 (same inputs as v0.5.1 incl. trusted-signing-account-name). No pull_request_target/workflow_run workflows, so checkout v7 fork block does not apply. Only verifiable on CI.

## Verification
cargo fmt --check, cargo clippy --workspace --all-targets -D warnings, cargo deny check, tests for bitacora-runtime, bitacora-mcp, bitacora-cli, xtask. bitacora-app tests not linkable locally (missing libxkbcommon-x11); `cargo check -p bitacora-app --all-targets` OK.

Related: [[release-process]], [[architecture]]
