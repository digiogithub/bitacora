---
id: BIT-T-0165
type: task
title: Write and run the manual IME and keymap checklist on Linux, macOS and Windows
status: in_review
priority: medium
parent: BIT-US-0031
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, test, ime]
estimate: 2
created: 2026-10-06T14:30:44Z
updated: 2026-10-06T21:12:28Z
started: 2026-10-06T21:12:28Z
---

## Description
Add `docs/design/ime-checklist.md` with steps for: Japanese (Mozc/macOS Kotoeri/MS-IME), Chinese Pinyin, Korean, dead keys (US-Intl), emoji picker, Enter-to-confirm vs split, Backspace during composition, and the main Logseq shortcuts. Run it on the three OSes and record results in the story comments.

## Acceptance Criteria
- Checklist committed and linked from [[block-editor]] §7.2.
- Results for all three OSes recorded; failures filed as bugs.

## Notes
Story BIT-US-0031. Verifies BIT-SP-0004.R20, BIT-SP-0004.R21. AGENTS.md §6 (manual IME checklist).
