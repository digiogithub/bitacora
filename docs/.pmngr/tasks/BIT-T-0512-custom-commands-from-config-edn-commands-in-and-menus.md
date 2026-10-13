---
id: BIT-T-0512
type: task
title: Custom commands from config.edn :commands in / and < menus
status: done
priority: medium
parent: BIT-US-0188
milestone: BIT-M-0006
author: mcp
estimate: 3
created: 2026-10-13T07:57:55Z
updated: 2026-10-13T08:17:37Z
started: 2026-10-13T08:09:22Z
closed: 2026-10-13T08:17:37Z
---

## Description
Read `:commands` from `logseq/config.edn` (`bitacora-config`, comment-preserving reader; Logseq `state.cljs:522`). Each `[name value]` pair with a string value is appended to both the slash and angle menus (`commands.cljs:214`, `:310`) and inserts `value` verbatim, replacing the trigger + query (`handler/editor.cljs:1369-1387`). Vector (step) values are ignored with a log warning. Menu refreshes when config.edn changes (watcher).

## Acceptance Criteria
- `bitacora-config` test: parse `:commands [["js" "Javascript"]]`, invalid entries skipped.
- `editor/commands.rs`: command list = built-ins + custom; text tests for insertion.
- `#[gpui::test]`: typing `<js` shows the custom entry and Enter inserts `Javascript`.
