---
id: BIT-US-0136
type: story
title: Pando settings model and keychain credentials
status: done
priority: high
parent: BIT-EP-0020
milestone: BIT-M-0007
author: mcp
labels: [v2, pando, settings, security, bitacora-config]
estimate: 3
created: 2026-10-07T09:15:49Z
updated: 2026-10-07T10:29:46Z
closed: 2026-10-07T10:29:46Z
---

## Description
As a user, I want my Pando connection settings stored on this machine and my tokens in the OS keychain, so that nothing sensitive ends up in my graph or git.

## Acceptance Criteria
- Machine-local settings: enabled, mode, REST URL, AG-UI URL, allow_remote, profile per feature, feature switches, per-graph consent records and exclusions.
- URL validation: loopback only unless `allow_remote`; non-loopback requires `https`.
- Tokens in keychain (reuse existing keyring backend) with env-var override; redacted `Debug`; never logged (test with trace logging).

## Notes
Implements BIT-SP-0009.R2, BIT-SP-0009.R3. Reference: git-in-track `internal/config/pando.go`, `pandomode.go`; Bitacora `credentials.rs`, `tokens.rs` keyring backend.
