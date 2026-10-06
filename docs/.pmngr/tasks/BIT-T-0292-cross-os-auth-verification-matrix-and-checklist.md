---
id: BIT-T-0292
type: task
title: Cross-OS auth verification matrix and checklist
status: backlog
priority: medium
parent: BIT-US-0046
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, auth, testing, docs]
estimate: 2
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T14:32:57Z
---

## Description
Write `docs/design/git-sync-auth-checklist.md` (linked from [[git-sync-merge]]) and execute it: Linux (ssh-agent, `~/.ssh/config` Host alias + IdentityFile, libsecret helper), macOS (ssh-agent/Keychain, osxkeychain helper, 1Password agent), Windows (OpenSSH agent pipe, Pageant, Git Credential Manager OAuth for GitHub). Record results; add a CI job on 3 OSes that fetches/pushes over SSH to a local `sshd` test container (Linux only) to cover the CLI env setup.

## Acceptance Criteria
- Checklist committed with results per OS; CI SSH job green on Linux.

## Notes
Story BIT-US-0046. Verifies BIT-SP-0006.R6. ADR-007.
