---
id: BIT-US-0084
type: story
title: Task markers, priorities, SCHEDULED/DEADLINE and LOGBOOK parsing
status: done
priority: high
parent: BIT-EP-0003
milestone: BIT-M-0002
author: mcp
labels: [markdown, compat, parser, tasks]
estimate: 5
created: 2026-10-06T14:29:34Z
updated: 2026-10-06T17:23:44Z
closed: 2026-10-06T17:23:44Z
---

## Description
As a Bitacora user who manages tasks in Logseq, I want TODO/DOING/NOW/LATER markers, `[#A]` priorities, scheduled/deadline dates with repeaters, and `:LOGBOOK:` clock entries parsed exactly like Logseq, so that my task lists, agenda and time tracking agree in both apps and Bitacora writes the same formats when it changes them.

## Acceptance Criteria
- Markers recognised only right after the bullet or heading hashes and followed by a space (`- LATER` alone is not a marker, mldoc 1.5.7); `TODOx` is not a marker.
- `[#X]` priority right after marker or bullet.
- `SCHEDULED: <2024-01-01 Mon .+1d>` → `20240101`, repeater Dotted/1/day; `+`, `++`, `.+` and units `h d w m y` supported; inactive `[…]` recognised.
- `:LOGBOOK:` drawer with open `CLOCK: [2024-01-01 Mon 10:00:00]` and closed `CLOCK: [..]--[..] =>  01:00:00` entries parsed; formatter emits the exact same strings.
- Fixtures 10–12 of [[02-markdown-block-syntax]] §11 pass.

## Notes
Implements: BIT-SP-0001.R9, BIT-SP-0001.R10.
See [[02-markdown-block-syntax]] §5.3; Logseq `marker.cljs:6-58`, `clock.cljs:69-93`, `mldoc inline.ml:1092-1150`.
