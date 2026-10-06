---
id: BIT-T-0295
type: task
title: Sync settings page and CLI sync/doctor commands
status: backlog
priority: low
parent: BIT-US-0047
milestone: BIT-M-0004
author: mcp
labels: [bitacora-app, bitacora-config, sync, settings]
estimate: 2
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T14:32:57Z
---

## Description
Expose `sync.*` settings (enabled, remote, branch, commit_idle_secs, commit_max_secs, fetch intervals, squash_auto_commits, author, collapsed_policy, git_binary) in `crates/bitacora-app/src/settings/sync.rs`, persisted via `bitacora-config` and hot-applied to the engine. `bitacora-cli sync --graph <p>` runs one commit→sync cycle headless and prints the result; `doctor` reports git version, remote reachability, attributes/gitignore presence.

## Acceptance Criteria
- Validation clamps (idle 5–600 s); CLI integration test against temp bare repo.

## Notes
Story BIT-US-0047. See [[git-sync-merge]] §7.
