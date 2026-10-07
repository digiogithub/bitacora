---
id: BIT-T-0492
type: task
title: "Fix: title-bar search field overlaps other controls in narrow windows"
status: done
priority: high
parent: BIT-US-0128
author: mcp
labels: [bug, v2, responsive]
created: 2026-10-07T15:54:11Z
updated: 2026-10-07T15:58:21Z
started: 2026-10-07T15:54:11Z
closed: 2026-10-07T15:58:21Z
---

## Description
Owner report 2026-10-07: in a narrow window the title-bar search field overlaps tabs, buttons and window controls. Cause: fixed-width search in a flex centre slot that could shrink to 0 and spill over the right slot.

## Acceptance Criteria
- Slots clip their own content and never overlap; Medium shrinks the search; Narrow collapses it to an icon button and drops theme/PDF buttons; window controls and drag area stay visible.
- gpui test at 1400/1000/760/640/480 asserts slot bounds do not overlap.
