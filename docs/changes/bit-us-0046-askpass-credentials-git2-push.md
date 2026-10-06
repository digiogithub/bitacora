---
created_at: 2026-10-06T18:35:33.664028116Z
updated_at: 2026-10-06T18:35:33.664028116Z
tags:
    - change
    - sync
    - auth
---
# BIT-US-0046 / ADR-023: libgit2 push, credential provider, askpass bridge

Continues [[bitacora-full-development-plan]], [[git-sync-merge]] (section 3.1), [[changes/bit-us-0041-0043-git-backends-and-onboarding]].

## What changed (crate bitacora-sync, commit 966d7d4)
- `backend/git2_push.rs` (feature `git2-push`, default): `push()` via git2 for non-local remotes; `GixBackend::push_via_libgit2`, `with_credentials`. Never forced; NonFastForward / Auth / Network mapping.
- `credentials.rs`: `Secret`, `CredentialProvider`, `ChainProvider`, `SecretStore` (`MemoryStore`, `KeyringStore` feature `keyring-store`), `PromptHandler` (UI seam).
- `askpass.rs` + `src/bin/askpass.rs` (`bitacora-askpass`): `AskpassServer`, `ProviderAskpass`, `request_answer`, `helper_main`; `CliConfig::askpass_env`, `SSH_ASKPASS`.
- Root `Cargo.toml` git2 pin (vendored libgit2 + OpenSSL), `deny.toml` comment (no exception needed; Rust crates are MIT/Apache, C GPL-2.0-with-exception documented), design doc 3.1 + checklist.

## Verification
fmt, workspace clippy -D warnings clean, `cargo deny check` ok, `cargo xtask check-deps` ok, `cargo test -p bitacora-sync` all green (44 unit + backends/auto_commit/onboarding/sync_engine + new git2_push (4) and askpass (1)), `--no-default-features` builds.

## Left
BIT-T-0291 app modal (bitacora-app) and manual macOS/Windows matrix (BIT-T-0292, in_review).
