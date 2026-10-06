---
id: BIT-T-0311
type: task
title: Slash command menu with Logseq catalogue
status: backlog
priority: medium
parent: BIT-US-0105
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, editor]
estimate: 3
created: 2026-10-06T14:33:07Z
updated: 2026-10-06T14:33:07Z
---

## Description
`crates/bitacora-app/src/editor/commands/slash.rs`: typing `/` in a BlockEditor opens a caret-anchored `Popover` + `List` (reuse the autocomplete infrastructure from BIT-US-0038) with commands: TODO/DOING/LATER/NOW/DONE, Heading 1–6, Page reference, Block reference, Page/Block embed, Query, Date (today/tomorrow/yesterday/pick), Scheduled, Deadline, Template, Code block, Link, Image/asset link, Number list/children, Draw (excluded). `nucleo` fuzzy filter; each command produces a core `Op` transaction.

## Acceptance Criteria
- `#[gpui::test]`: `/todo` + Enter prefixes `TODO `; `/query` inserts `{{query }}` with caret inside.
- Esc closes and leaves the `/` text.

## Notes
[[04-editor-outliner-operations]] §4; [[block-editor]] §9 Later.
