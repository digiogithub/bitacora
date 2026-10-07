---
id: BIT-US-0127
type: story
title: Popovers, modals, palette and settings window restyle
status: backlog
priority: medium
parent: BIT-EP-0017
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, bitacora-app]
estimate: 5
created: 2026-10-07T09:13:09Z
updated: 2026-10-07T09:13:09Z
---

## Description
As a user, I want dialogs to match the design: PDF export popover, command palette, settings window (with sections ready for Pando), conflict resolver, confirmation dialogs, toasts.

## Acceptance Criteria
- All use tokens and components only (lint passes); keyboard behaviour unchanged; existing dialog tests pass.

## Notes
Files: `crates/bitacora-app/src/views/settings/`, modal views.
