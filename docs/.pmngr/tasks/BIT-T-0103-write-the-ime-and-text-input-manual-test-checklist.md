---
id: BIT-T-0103
type: task
title: Write the IME and text-input manual test checklist
status: backlog
priority: critical
parent: BIT-US-0072
milestone: BIT-M-0001
author: mcp
labels: [spike, ime, docs, bitacora-app]
estimate: 1
created: 2026-10-06T14:29:45Z
updated: 2026-10-06T14:29:45Z
---

## Description
Create `docs/design/ime-test-checklist.md`: a matrix of platforms (macOS 14+, Windows 11, Linux X11/Wayland on GNOME and KDE, fcitx5 and ibus) x cases: composition start/commit/cancel, candidate window position at caret (also on wrapped second row and near window edge), composition across block navigation (must commit or cancel cleanly), Backspace during composition, dead keys (`´` + `e`), emoji picker, CJK + emoji selection by mouse, HiDPI fractional scaling, paste during composition. Each case has steps and expected result, and a results table template (OS, version, IME, pass/fail, notes). Use `[[wikilinks]]` to [[block-editor]] and [[gpui-and-gpui-kit]].

## Acceptance Criteria
- Checklist covers every platform/IME combination listed in [[gpui-and-gpui-kit]] §3.2 item 7.
- Reviewed by one person who uses a CJK IME daily (note in PR).

## Notes
- AGENTS.md §6 (manual IME checklist per OS). [[gpui-and-gpui-kit]] §1.2, Risk R4.
