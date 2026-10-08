---
id: BIT-US-0176
type: story
title: Git sync over HTTPS asks for credentials and stores them in the system keychain
status: in_review
priority: high
parent: BIT-EP-0026
milestone: BIT-M-0010
author: mcp
labels: [bitacora-sync, bitacora-app, git, security]
created: 2026-10-08T12:23:05Z
updated: 2026-10-08T12:57:16Z
started: 2026-10-08T12:57:16Z
---

## Description
HTTPS remotes fail silently: no credential prompt. Bitacora must ask for username/token when the remote needs auth and store them in the OS keychain (keyring), then supply them to git CLI and the git2 push fallback.

## Acceptance Criteria
- Auth failure on HTTPS surfaces a credential dialog (username + password/token).
- Credentials saved in the system keychain keyed by remote host+graph; never written to disk in plain text or to git config.
- git CLI receives credentials through a non-interactive mechanism (askpass/credential helper env), never in the URL or argv.
- Settings can forget stored credentials.
- Integration test with a credential-requiring fake remote or unit tests of the askpass plumbing.
