---
id: BIT-US-0074
type: story
title: "Read-only block rendering: refs, tags, markers, properties, code and images"
status: in_progress
priority: high
parent: BIT-EP-0006
milestone: BIT-M-0002
author: mcp
labels: [ui, rendering, bitacora-app]
estimate: 8
created: 2026-10-06T14:29:26Z
updated: 2026-10-06T18:28:35Z
started: 2026-10-06T18:28:35Z
---

## Description
As a reader, I want blocks to render like in Logseq (clickable `[[page]]` and `#tag`, resolved `((block refs))`, TODO checkboxes, priority badges, SCHEDULED/DEADLINE chips, properties table, code blocks, images, collapse arrows), so that my graph is readable and navigable.

## Acceptance Criteria
- Inline runs from the parsed inline AST: text, bold/italic/strike, inline code, links, page refs, tags, block refs (resolved title, dangling shown as raw), embeds and `{{query}}` as placeholders (results in BIT-EP-0013).
- Block decorations: marker (checkbox for TODO/DONE), priority badge, timestamps, properties table hiding hidden built-ins, collapsed LOGBOOK, heading sizes.
- Code fences rendered with syntax highlighting; images from `../assets/` loaded asynchronously.
- All fixture pages render without panics (snapshot test runs over every fixture page).

## Notes
See [[04-editor-outliner-operations]] §8, [[block-editor]] §8, [[gpui-and-gpui-kit]] §2.3 ("Inline rendering of refs"). ADR-002, ADR-003.
