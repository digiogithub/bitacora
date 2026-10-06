---
id: BIT-T-0374
type: task
title: "gix fallback credentials: HTTPS via OS keyring and in-app prompt, SSH via gix transport"
status: done
priority: high
parent: BIT-US-0046
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, auth, gix]
estimate: 3
created: 2026-10-06T15:15:10Z
updated: 2026-10-06T18:35:25Z
closed: 2026-10-06T18:35:25Z
---

## Description
`crates/bitacora-sync/src/backend/gix_auth.rs`: credential provider for the gix-only backend (ADR-020).
- HTTPS: look up `(host, username)` in the OS keyring (`keyring` crate: Keychain, Credential Manager, Secret Service); if missing or rejected, ask through the same `CredentialPrompter` used by the askpass bridge (BIT-T-0290/BIT-T-0291) and offer "remember in keychain"; erase stored secret on auth failure.
- SSH: use the gix SSH transport (system `ssh` program when available, honouring `~/.ssh/config`; passphrase prompts routed to `CredentialPrompter`); report a clear `GitError::Auth` when no key/agent is usable.
- On auth failure while the gix backend is active, attach a hint "Install git to use your system SSH/credential configuration" to the error.
Secrets never logged.

## Acceptance Criteria
- Test: HTTPS basic-auth test server; credential from a mock keyring succeeds; missing credential → fake prompter answers; wrong credential → erased + `GitError::Auth` with install-git hint.
- Manual check of SSH via gix transport on Linux, macOS and Windows added to the auth checklist (BIT-T-0292).

## Notes
Story BIT-US-0046. Implements BIT-SP-0006.R6. ADR-020.
