---
id: BIT-T-0513
type: task
title: Full-width 《〈 opens the < menu
status: backlog
priority: low
parent: BIT-US-0188
milestone: BIT-M-0006
author: mcp
estimate: 1
created: 2026-10-13T07:57:55Z
updated: 2026-10-13T07:57:55Z
---

## Description
Logseq replaces a typed `《〈` with `<` and opens the block-commands menu (`handler/editor.cljs:3010-3018`). Mirror it in `editor/completion.rs` / the editor input handler. Also document the intentional word-start divergence of `command_trigger` in `docs/design/block-editor.md` §7.4.

## Acceptance Criteria
- Unit test: `《〈` -> `<` + `Trigger::Angle`; `a<b` still does not open the menu.
