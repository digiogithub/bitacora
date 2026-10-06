---
id: BIT-T-0355
type: task
title: Task marker combination and SCHEDULED/DEADLINE line merge
status: backlog
priority: high
parent: BIT-US-0050
milestone: BIT-M-0003
author: mcp
labels: [bitacora-merge, merge, tasks]
estimate: 2
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T15:17:52Z
---

## Description
`crates/bitacora-merge/src/marker.rs`: split marker (`TODO|DOING|NOW|LATER|WAITING|DONE|CANCELED|CANCELLED`) + priority from first line before content diff3; marker-only vs text-only → combine; both markers changed with identical text → rank `DONE > CANCELED > DOING/NOW > TODO/LATER/WAITING`; both changed text and marker → content conflict. `SCHEDULED:` / `DEADLINE:` lines merged per line 3-way, divergent → `Conflict::Property{key:"SCHEDULED"}`.

## Acceptance Criteria
- Tests for the three scenarios in BIT-SP-0006.R13 plus repeater syntax (`.+1d`) preservation.

## Notes
Story BIT-US-0050. Implements BIT-SP-0006.R13.
ADR-016: lives in the `bitacora-merge` crate (depends only on `bitacora-markdown`), shared by `bitacora-core` (external edits, BIT-US-0069) and `bitacora-sync`.
Needed by BIT-US-0069 (ADR-016).
