---
id: BIT-T-0150
type: task
title: Timestamp parser and SCHEDULED/DEADLINE lines
status: backlog
parent: BIT-US-0084
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, parser, tasks]
estimate: 2
created: 2026-10-06T14:30:16Z
updated: 2026-10-06T14:30:16Z
---

## Description
Implement `crates/bitacora-markdown/src/timestamp.rs`: parse `<YYYY-MM-DD Www[ HH:MM][ repeater]>` (active) and `[…]` (inactive) with repeaters `+Nu` (Plus), `++Nu` (DoublePlus), `.+Nu` (Dotted), `u ∈ h d w m y` (`mldoc inline.ml:1092-1150`). Detect `SCHEDULED: <…>` / `DEADLINE: <…>` body lines (own line, possibly after continuation indent) and expose `scheduled`/`deadline` as `yyyyMMdd` ints plus `repeated: bool` (`block.cljs:240-271`). Provide `Timestamp::format()` that writes Logseq's exact text (`src/main/frontend/util/text.cljs:35-59`) with uppercase keys.

## Acceptance Criteria
- Fixture 11 of §11: `+1w`, `++1d`, `.+1m` repeaters and an inactive timestamp parse; format(parse(s)) == s.
- `SCHEDULED: <2024-01-01 Mon .+1d>` → 20240101, repeated, Dotted/1/Day.
- Weekday text is preserved as written (not recomputed) when unchanged.

## Notes
Part of BIT-US-0084. Implements BIT-SP-0001.R10.
