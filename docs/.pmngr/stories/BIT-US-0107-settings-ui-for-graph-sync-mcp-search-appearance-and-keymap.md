---
id: BIT-US-0107
type: story
title: Settings UI for graph, sync, MCP, search, appearance and keymap
status: done
priority: high
parent: BIT-EP-0013
milestone: BIT-M-0005
author: mcp
labels: [ui, settings, bitacora-app, bitacora-config]
estimate: 5
created: 2026-10-06T14:31:46Z
updated: 2026-10-06T21:53:58Z
started: 2026-10-06T21:19:44Z
closed: 2026-10-06T21:53:58Z
---

## Description
As a user, I want a settings window to change graph options, sync, the MCP server, search/index options, appearance and keybindings, with changes applied immediately where possible, so that I never have to hand-edit config files.

## Acceptance Criteria
- Sections: General (graph path, journals format via `config.edn`), Editor, Search & Index (`search.substring`, `max_block_len`, `paranoid_scan`, reindex button), Sync (remote, interval, auto-commit), MCP (enabled, port, token regenerate, writes toggle), Appearance (theme, font size), Keymap.
- `config.edn` edits are comment-preserving via `bitacora-config`; app settings stored in the platform config dir.
- Changes apply without restart except where explicitly labelled "restart required".
- Settings that invalidate the index (e.g. journal title format) warn and trigger reindex.

## Notes
Implements: BIT-SP-0003.R2, BIT-SP-0003.R15 (settings side). [[gpui-and-gpui-kit]] §2.2 (Settings, Form components). ADR-010 (MCP defaults).
