---
id: BIT-US-0057
type: story
title: Today's journal auto-creation (virtual until edited)
status: in_progress
priority: high
parent: BIT-EP-0009
milestone: BIT-M-0003
author: mcp
labels: [core, compat, journals]
estimate: 3
created: 2026-10-06T14:28:36Z
updated: 2026-10-06T19:13:32Z
started: 2026-10-06T19:13:32Z
---

## Description
As a user, I want today's journal to be ready when I open the app, but without a file being written until I type, so that I can start writing immediately and my graph (and git history) doesn't fill with empty `-` journal files.

Logseq creates today's journal page on app open when `:feature/enable-journals?` is true (`handler/page.cljs:820-852`, `handler/repo.cljs:80-113`), inserting the `:default-templates {:journals "name"}` template. Bitacora keeps the page virtual until the first non-blank edit; then the file is `<:journals-directory>/<:journal/file-name-format>.md`. A journal whose trimmed content equals `-` or the template is treated as empty (watcher rule `fs/watcher_handler.cljs:91-102`).

## Acceptance Criteria
- Opening the app on 2025-11-14 shows the journal `Nov 14th, 2025`; `journals/2025_11_14.md` does not exist until the user types.
- After typing `hello`, the file is `journals/2025_11_14.md` with `- hello`.
- `:journals-directory "daily"` + `:journal/file-name-format "yyyy-MM-dd"` → `daily/2025-11-14.md`.
- `:feature/enable-journals? false` → no journal is auto-created.
- With `:default-templates {:journals "daily"}` the template block is shown; the file is written only once the user changes content beyond the template.
- Date rollover while the app is open creates the new day's virtual journal.

## Notes
Implements: BIT-SP-0002.R11, BIT-SP-0002.R12
See [[01-file-graph-layout]] §4 and open question 1; [[04-editor-outliner-operations]]. ADR-013.
