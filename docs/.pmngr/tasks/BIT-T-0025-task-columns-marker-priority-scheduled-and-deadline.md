---
id: BIT-T-0025
type: task
title: "Task columns: marker, priority, SCHEDULED and DEADLINE"
status: done
priority: high
parent: BIT-US-0005
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 2
created: 2026-10-06T14:26:33Z
updated: 2026-10-06T17:32:45Z
closed: 2026-10-06T17:32:45Z
---

## Description
`crates/bitacora-index/src/derive/tasks.rs`: extract `marker` (TODO DOING DONE LATER NOW WAIT WAITING CANCELED CANCELLED IN-PROGRESS), `priority` (A/B/C from `[#A]`), `scheduled`/`deadline` as `yyyyMMdd` ints plus `*_raw` (full `<2026-10-06 Tue 10:00 .+1w>` text) and `repeated` when a repeater (`+`, `++`, `.+`) is present.

## Acceptance Criteria
- Tests: marker only at block start; `SCHEDULED: <2026-10-06 Tue>` → `20261006`; repeater sets `repeated = 1`; lowercase `todo` is not a marker.
- `tasks` view returns exactly blocks with a marker on a fixture page.

## Notes
BIT-SP-0003.R3. [[03-parsing-indexing-search]] §3.3.
