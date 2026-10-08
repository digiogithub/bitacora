---
id: BIT-US-0177
type: story
title: "Git sync over SSH: choose the private key when system keys fail"
status: done
priority: medium
parent: BIT-EP-0026
milestone: BIT-M-0010
author: mcp
labels: [bitacora-sync, bitacora-app, git]
created: 2026-10-08T12:23:05Z
updated: 2026-10-08T14:36:13Z
started: 2026-10-08T12:57:16Z
closed: 2026-10-08T14:36:13Z
---

## Description
Let the user pick an SSH private key file for the graph's remote, used when the system agent/default keys do not work.

## Acceptance Criteria
- Sync settings have an optional "SSH key" file field (per graph).
- When set, git runs with `GIT_SSH_COMMAND=ssh -i <key> -o IdentitiesOnly=yes` (path quoted) and the git2 fallback uses the same key.
- SSH auth failure message suggests setting the key.
- Passphrase-protected keys: passphrase asked and stored in keychain, or documented limitation.
