---
id: BIT-T-0355
type: task
title: Task marker combination and SCHEDULED/DEADLINE line merge
status: backlog
priority: high
parent: BIT-US-0050
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, merge, tasks]
estimate: 2
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T14:34:49Z
---

## Description
`crates/bitacora-sync/src/merge/marker.rs`: split marker (`TODO|DOING|NOW|LATER|WAITING|DONE|CANCELED|CANCELLED`) + priority from first line before content diff3; marker-only vs text-only → combine; both markers changed with identical text → rank `DONE > CANCELED > DOING/NOW > TODO/LATER/WAITING`; both changed text and marker → content conflict. `SCHEDULED:` / `DEADLINE:` lines merged per line 3-way, divergent → `Conflict::Property{key:"SCHEDULED"}`.

## Acceptance Criteria
- Tests for the three scenarios in BIT-SP-0006.R13 plus repeater syntax (`.+1d`) preservation.

## Notes
Story BIT-US-0050. Implements BIT-SP-0006.R13.
