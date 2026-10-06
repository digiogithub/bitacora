---
id: BIT-US-0046
type: story
title: In-app askpass credential bridge and cross-OS auth verification
status: backlog
priority: high
parent: BIT-EP-0011
milestone: BIT-M-0004
author: mcp
labels: [git, sync, auth, ui]
estimate: 5
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T14:28:30Z
---

## Description
As a user with an HTTPS remote and no credential helper (or a passphrase-protected key without agent), I want Bitacora to prompt me inside the app, so that sync never hangs on an invisible terminal prompt.

## Acceptance Criteria
- A small askpass helper (subcommand of the app/cli binary) is passed in `GIT_ASKPASS`/`SSH_ASKPASS`; it forwards the prompt to the running app over a local authenticated channel and prints the answer.
- The app shows a modal (username / password / passphrase); cancel → auth error state, no retry storm.
- Manual test matrix documented and executed: SSH agent + `~/.ssh/config` alias, HTTPS osxkeychain, libsecret, Git Credential Manager (Windows) — Linux, macOS, Windows.
- Secrets are never logged.

## Notes
Implements: BIT-SP-0006.R6. See [[git-sync-merge]] §3. ADR-007.
