---
id: BIT-T-0284
type: task
title: Idle debouncer with hard cap and commit step under the graph write lock
status: done
priority: high
parent: BIT-US-0044
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, commit]
estimate: 3
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T17:48:56Z
closed: 2026-10-06T17:48:56Z
---

## Description
`crates/bitacora-sync/src/autocommit.rs`: `CommitScheduler` fed by `FileFlushed{path, origin}` events (core writer + watcher); timers: idle `commit_idle_secs` (clamped 5 s–600 s) reset on each event, cap `commit_max_secs` from first dirty event; triggers on open/close/manual. `commit_step()`: acquire graph write lock, `git add -A -- pages journals logseq/config.edn logseq/custom.css assets whiteboards draws .gitignore` (respecting ignores), skip if nothing staged, determine kind (agent if all changes came from `Origin::Agent`; mixed → auto with `Bitacora-Agent` notes), commit. Uses injectable clock.

## Acceptance Criteria
- Unit tests with fake clock: idle fire at 20 s, cap at 300 s, ignored-only skip, agent kind detection.

## Notes
Story BIT-US-0044. Implements BIT-SP-0006.R1, BIT-SP-0006.R7, BIT-SP-0007.R7.
