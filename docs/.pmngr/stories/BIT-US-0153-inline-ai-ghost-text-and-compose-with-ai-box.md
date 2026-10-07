---
id: BIT-US-0153
type: story
title: Inline AI ghost text and Compose with AI box
status: in_review
priority: low
parent: BIT-EP-0023
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, editor, bitacora-app]
estimate: 8
created: 2026-10-07T09:18:16Z
updated: 2026-10-07T12:50:53Z
started: 2026-10-07T12:50:47Z
---

## Description
As a writer, I want AI help inside the editor as the design shows: a ⌘J "Compose with AI" box and inline ghost-text continuations, which never insert anything unless I accept.

## Acceptance Criteria
- ⌘J opens the compose box anchored to the block (prompt, context chips, streaming amber preview, Accept/Replace/Discard).
- Inline ghost text (opt-in): after a pause, amber continuation after the caret with hint bar; Tab accepts, Escape/typing discards.
- Accepted text is a normal undoable edit; IME composition never interrupted (IME checklist updated).

## Notes
Implements BIT-SP-0011.R6. Design: `docs/componentes.md` (inline AI, Compose box), `docs/interaccion.md`. Profile `bitacora-writer`.
