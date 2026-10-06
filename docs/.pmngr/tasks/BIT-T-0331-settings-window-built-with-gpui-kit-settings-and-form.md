---
id: BIT-T-0331
type: task
title: Settings window built with GPUI Kit Settings and Form components
status: done
priority: high
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
`crates/bitacora-app/src/views/settings/`: GPUI Kit `Settings` page with sidebar sections General, Editor, Search & Index, Sync, MCP, Appearance, Keymap using `Switch`, `Select`, `NumberInput`, `Input`, `Button`. Live-applied changes take effect immediately; `ReindexRequired` shows a confirm `Dialog` then triggers reindex; `RestartRequired` shows a badge. MCP section: regenerate token (copy button), port, writes toggle with warning text.

## Acceptance Criteria
- `#[gpui::test]`: toggling theme applies live; changing journal title format asks to reindex; values persist across restart.

## Notes
[[gpui-and-gpui-kit]] §2.2 (Settings, Form). ADR-010.
