---
id: BIT-T-0467
type: task
title: Inline ghost-text continuation
status: in_review
priority: low
parent: BIT-US-0153
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, editor, ime]
estimate: 3
created: 2026-10-07T09:19:06Z
updated: 2026-10-07T12:50:53Z
started: 2026-10-07T12:50:47Z
---

## Description
Opt-in: after idle pause, request continuation; render amber ghost text after caret with hint bar; Tab accepts, Escape/typing/caret move discards; never during IME composition.

## Acceptance Criteria
- Editor tests incl. IME composition guard; IME checklist updated.
