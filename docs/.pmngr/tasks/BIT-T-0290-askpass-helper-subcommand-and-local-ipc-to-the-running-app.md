---
id: BIT-T-0290
type: task
title: Askpass helper subcommand and local IPC to the running app
status: backlog
priority: high
parent: BIT-US-0046
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, bitacora-cli, auth]
estimate: 3
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T15:15:35Z
---

## Description
Add hidden subcommand `bitacora-cli askpass <prompt>` (also callable from the app binary). It connects to a per-session local endpoint (Unix domain socket in the app runtime dir / Windows named pipe via `interprocess` crate) whose path and one-time secret are passed in env (`BITACORA_ASKPASS_SOCK`, `BITACORA_ASKPASS_TOKEN`) set by `CliBackend`. Sends `{prompt}`, receives `{answer}` or `{cancel}`; prints answer to stdout, exit 1 on cancel. Server side in `crates/bitacora-sync/src/askpass.rs` forwards prompts to a `CredentialPrompter` trait (UI implements). 120 s timeout. The askpass bridge only applies to the system-git backend; the gix-only backend calls the same `CredentialPrompter` in-process (BIT-T-0374).

## Acceptance Criteria
- Test: fake prompter answers; `git` fetch over a local HTTP remote requiring basic auth (test server) succeeds via askpass; cancel → `GitError::Auth`.
- Secrets never logged.

## Notes
Story BIT-US-0046. Implements BIT-SP-0006.R6. ADR-020.
