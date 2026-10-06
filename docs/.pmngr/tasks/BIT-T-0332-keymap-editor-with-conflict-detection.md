---
id: BIT-T-0332
type: task
title: Keymap editor with conflict detection
status: done
priority: medium
parent: BIT-US-0107
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, settings, ui]
estimate: 3
created: 2026-10-06T14:34:02Z
updated: 2026-10-06T21:53:47Z
closed: 2026-10-06T21:53:47Z
---

## Description
`crates/bitacora-app/src/views/settings/keymap.rs`: `DataTable` of all registered actions (context, default binding, user binding) with search; record-a-shortcut cell; conflict detection within the same key context; reset to Logseq defaults; persisted in `AppSettings.keymap` and re-bound live via `cx.bind_keys`.

## Acceptance Criteria
- `#[gpui::test]`: rebinding an action takes effect immediately; conflicting binding shows a warning and is not saved until confirmed.

## Notes
[[04-editor-outliner-operations]] §7, Requirements 18; [[block-editor]] §9 Later ("customizable keymap UI").
