---
id: BIT-T-0516
type: task
title: Paste a URL over selected text to make a Markdown link
status: done
priority: medium
author: mcp
labels: [editor, bitacora-app, bitacora-core, logseq-compat]
created: 2026-11-11T20:32:59Z
updated: 2026-11-11T20:59:04Z
started: 2026-11-11T20:32:59Z
closed: 2026-11-11T20:59:04Z
---

## Description
Owner request (2026-11-11): pasting a URL while text is selected in the edited block must turn the selection into a Markdown link `[selected text](url)`, as Logseq 0.10.x does (`src/main/frontend/handler/paste.cljs:137-146`). Implements part of BIT-SP-0004.R13 (paste).

## Acceptance Criteria
- URL pasted over a non-empty selection gives `[selection](url)`; caret lands after the link.
- Text pasted over a selected URL gives `[text](url)`.
- URL over URL, and a selection that is already the label or target of a link, stay plain replacements.
- Raw paste (`Mod+Shift+V`) is never converted.

## Notes
`bitacora_core::editor::link_paste` (pure) + `OutlineEditor::paste` in `crates/bitacora-app/src/editor/view.rs`.
