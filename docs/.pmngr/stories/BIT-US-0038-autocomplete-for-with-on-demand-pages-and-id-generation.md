---
id: BIT-US-0038
type: story
title: "Autocomplete for [[ # (( with on-demand pages and id:: generation"
status: in_progress
priority: high
parent: BIT-EP-0007
milestone: BIT-M-0003
author: mcp
labels: [editor, autocomplete, bitacora-app, bitacora-core]
estimate: 8
created: 2026-10-06T14:28:07Z
updated: 2026-10-06T20:22:06Z
started: 2026-10-06T20:22:06Z
---

## Description
As a networked-notes user, I want typing `[[`, `#` or `((` to pop up page or block suggestions and insert the right link, creating pages on demand and persisting `id::` on referenced blocks, so that linking is fast and references never break.

## Acceptance Criteria
- `EditorAction` state machine (`PageSearch { start, hashtag }`, `BlockSearch { start }`) with triggers per `handle-last-input`.
- Popover anchored to the caret rect, fed by the index search (pages fuzzy excluding current page; blocks full-text limit 20 excluding self and ancestors).
- Choosing replaces the query range: `[[Page]]`, `#tag`, `#[[multi word]]`, `((uuid))`; "New page" entry creates the page entry.
- `EnsureUuid { id }` inserts `id:: <uuid>` after the title line in the same transaction as the `((uuid))` insert; no `id::` for unreferenced blocks.
- `Mod+C` while editing with no text selection copies `((uuid))`; `Mod+E` copies `{{embed ((uuid))}}`.
- Popup keys: Enter, Up/Down, Ctrl+P/N, Esc (closes popup only).

## Notes
Implements: BIT-SP-0004.R15, BIT-SP-0004.R16.
See [[block-editor]] §7.4, [[04-editor-outliner-operations]] §4 (`editor.cljs:1875-1936`, `:1944-1966`), [[sqlite-index-schema]] for search. ADR-006.
