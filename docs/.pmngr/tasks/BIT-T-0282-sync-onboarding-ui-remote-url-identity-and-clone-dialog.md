---
id: BIT-T-0282
type: task
title: "Sync onboarding UI: remote URL, identity and clone dialog"
status: done
priority: medium
parent: BIT-US-0043
milestone: BIT-M-0004
author: mcp
labels: [bitacora-app, sync, onboarding, ui]
estimate: 2
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T20:37:17Z
started: 2026-10-06T19:59:19Z
closed: 2026-10-06T20:37:17Z
---

## Description
`crates/bitacora-app/src/settings/sync.rs` + welcome screen action "Open graph from git remote": remote URL field (validate via `ls-remote` with progress), branch, author name/email, device name, enable toggle; clone dialog with destination folder picker. Errors (auth/network) shown with actionable text.

## Acceptance Criteria
- `#[gpui::test]` for form validation states; manual checklist for clone over SSH and HTTPS.

## Notes
Story BIT-US-0043. Implements BIT-SP-0006.R3. ADR-001.
