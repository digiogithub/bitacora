---
id: BIT-T-0249
type: task
title: "collapsed:: toggle, SCHEDULED/DEADLINE setters and LOGBOOK append"
status: done
parent: BIT-US-0093
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, serializer, tasks]
estimate: 2
created: 2026-10-06T14:32:01Z
updated: 2026-10-06T17:08:48Z
started: 2026-10-06T17:02:01Z
closed: 2026-10-06T17:08:48Z
---

## Description
Add `crates/bitacora-markdown/src/edit/state.rs`:
- `set_collapsed(content, bool, mode: CollapseMode)`: `InFile` → `collapsed:: true` via `set_property` / remove line on expand (`core.cljs:20-32`); `AppOnly` → returns content unchanged (state held in app settings). Existing `collapsed::` lines are always preserved when not toggled.
- `set_scheduled(content, Option<Timestamp>)` / `set_deadline(...)`: replace the existing line or insert directly after the title line (`src/main/frontend/util/text.cljs:35-59`).
- `clock_in(content, now)` / `clock_out(content, now)`: insert or update a `:LOGBOOK:` drawer after title, SCHEDULED/DEADLINE and properties (`drawer.cljs:31-86`), using the exact CLOCK formats from the drawer module; merge into an existing logbook (`drawer.cljs:126-140`).
- `set_marker(content, Option<Marker>)` rewrites only the marker token on the head line.

## Acceptance Criteria
- Collapse/expand of `"- collapsed parent\n\t- hidden child"` round-trips to the original bytes.
- Fixture 17 of §11 (collapsed blocks) round-trips untouched.
- Scheduling + id + clock in/out on `- TODO [#A] Parent block #tag` yields the order title, SCHEDULED, id, LOGBOOK (as in §7 canonical example).

## Notes
Part of BIT-US-0093. Implements BIT-SP-0001.R13, R15, R10.
