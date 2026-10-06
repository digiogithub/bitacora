---
id: BIT-T-0241
type: task
title: "Update service: release check, channels and settings"
status: backlog
priority: medium
parent: BIT-US-0100
milestone: BIT-M-0005
author: mcp
labels: [auto-update, bitacora-app]
estimate: 3
created: 2026-10-06T14:31:45Z
updated: 2026-10-06T14:31:45Z
---

## Description
Add `crates/bitacora-app/src/update/` with an `UpdateService` running on the tokio runtime: checks at most daily (persist `last_check` in settings) and on the "Check for updates…" action; channel `stable|beta` (beta = include pre-releases); backends `Velopack(UpdateManager with GithubSource)` and `Notice(GitHub Releases API via reqwest with rustls)` selected per the ADR. Emits `UpdateEvent::{Available{version, notes_url}, Downloaded, Error}` through the event channel. Settings: `updates.enabled` (default true), `updates.channel`. Respect proxies from env vars.

## Acceptance Criteria
- Unit tests with a mocked HTTP server (`wiremock` or axum test server) for: newer stable, only newer beta (ignored on stable), network error (logged, no UI error spam), disabled setting (no request).
- No request leaves the machine when updates are disabled.

## Notes
- [[crate-stack]] §4.1 (velopack, `self_update` not for GUI); [[gpui-and-gpui-kit]] §1.4 (tokio bridge).
