---
id: BIT-T-0151
type: task
title: Drawer and LOGBOOK CLOCK parser/formatter
status: backlog
parent: BIT-US-0084
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, parser, tasks]
estimate: 2
created: 2026-10-06T14:30:16Z
updated: 2026-10-06T14:39:30Z
---

## Description
Implement `crates/bitacora-markdown/src/drawer.rs`: generic drawers `:NAME:` … `:END:` on their own lines (case-insensitive end, `mldoc drawer.ml:84-103`) with spans; `:LOGBOOK:` content parsed into `ClockEntry::Open { start }` from `CLOCK: [2024-01-01 Mon 10:00:00]` and `ClockEntry::Closed { start, end, duration }` from `CLOCK: [start]--[end] =>  HH:MM:SS` (two spaces after `=>`, `clock.cljs:69-93`); unknown lines (e.g. `* State "DONE" from "TODO" [2024-01-01 Mon 10:00]`) kept as raw. Formatter reproduces the exact strings so clock-in/out written by Bitacora matches Logseq.

## Acceptance Criteria
- Fixture 12 of §11 (open + closed CLOCK) parses; formatting a closed entry yields `CLOCK: [2024-01-01 Mon 10:00:00]--[2024-01-01 Mon 11:00:00] =>  01:00:00`.
- Our own clock/duration vectors covering the documented cases, verified black-box against Logseq (no Logseq test file copied or translated).
- Drawer inside a fence is ignored.

## Notes
Part of BIT-US-0084. Implements BIT-SP-0001.R10. LOGBOOK is Metadata class (BIT-SP-0001.R16). ADR-015.
