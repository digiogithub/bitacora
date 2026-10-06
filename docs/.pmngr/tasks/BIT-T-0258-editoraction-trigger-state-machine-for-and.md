---
id: BIT-T-0258
type: task
title: "EditorAction trigger state machine for [[, # and (("
status: backlog
priority: high
parent: BIT-US-0038
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, autocomplete]
estimate: 2
created: 2026-10-06T14:32:23Z
updated: 2026-10-06T14:32:23Z
---

## Description
`crates/bitacora-app/src/editor/autocomplete/trigger.rs`: pure `on_input(buffer, caret, last_input, state) -> EditorAction` with `PageSearch { start, hashtag }` and `BlockSearch { start }` (Slash/Angle/Property states stubbed for later). Rules from `handle-last-input` (`editor.cljs:1875-1936`): `[[` after autopair, `#` at line start or after whitespace, `((`. The query is `buffer[start..caret]`; leaving the range or `Esc` closes the action.

## Acceptance Criteria
- Table tests: `see [[proj` → PageSearch query `proj`; `a#b` → no trigger; `x #tag` → hashtag; `((foo` → BlockSearch; caret moved before start closes.

## Notes
Story BIT-US-0038. Implements BIT-SP-0004.R15, BIT-SP-0004.R16.
