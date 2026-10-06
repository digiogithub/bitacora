---
id: BIT-T-0291
type: task
title: Credential prompt modal in the app
status: done
priority: high
parent: BIT-US-0046
milestone: BIT-M-0004
author: mcp
labels: [bitacora-app, auth, ui]
estimate: 2
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T20:37:17Z
started: 2026-10-06T19:59:19Z
closed: 2026-10-06T20:37:17Z
---

## Description
`crates/bitacora-app/src/views/credential_prompt.rs`: implements `CredentialPrompter` — GPUI Kit modal with prompt text from git ("Username for 'https://github.com'", "Password for …", "Enter passphrase for key …"), masked input for password/passphrase, Cancel. Works when window is hidden (bring app to front / tray notification). Cancel sets auth error state without automatic retry storm (engine waits for user Retry).

## Acceptance Criteria
- `#[gpui::test]` for masked vs plain prompt detection and cancel path.

## Notes
Story BIT-US-0046. Implements BIT-SP-0006.R6. ADR-001.
