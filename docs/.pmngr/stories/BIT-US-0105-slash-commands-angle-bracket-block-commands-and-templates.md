---
id: BIT-US-0105
type: story
title: Slash commands, angle-bracket block commands and templates
status: in_progress
priority: medium
parent: BIT-EP-0013
milestone: BIT-M-0005
author: mcp
labels: [ui, editor, bitacora-app, bitacora-core]
estimate: 8
created: 2026-10-06T14:31:46Z
updated: 2026-10-06T22:00:23Z
started: 2026-10-06T22:00:23Z
---

## Description
As a writer, I want `/` and `<` command menus, date picking, and templates (including the default journal template), so that I can insert structure quickly the way I do in Logseq.

## Acceptance Criteria
- `/` menu with the Logseq catalogue (TODO/DOING/LATER/NOW, headings, page/block ref, embeds, query, date pickers, scheduled/deadline, template, code block, image/asset link, calculator excluded) fuzzy-filtered.
- `<` menu: quote, src, note, tip, important, caution, warning, example, export, center, comment.
- `/date` and SCHEDULED/DEADLINE use GPUI Kit `DatePicker`/`Calendar` and write Logseq-format timestamps.
- Templates: blocks with `template:: name` listed; insertion expands `<% today %>`, `<% yesterday %>`, `<% tomorrow %>`, `<% time %>`, `<% current page %>` and `[[...]]` variables; `template-including-parent:: false` honoured; `:default-templates {:journals "..."}` applied to today's journal in memory.
- All insertions are single undoable transactions.

## Notes
[[block-editor]] §9 Later list; [[04-editor-outliner-operations]] §4 and Requirements 15. Builds on autocomplete from BIT-US-0038.
