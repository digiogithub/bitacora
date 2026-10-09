---
id: BIT-US-0184
type: story
title: AI journal review suggestions overflow the panel instead of scrolling
status: done
priority: high
parent: BIT-EP-0027
milestone: BIT-M-0011
author: mcp
labels: [bug, bitacora-app, ai, journals]
estimate: 2
created: 2026-10-09T11:23:31Z
updated: 2026-10-09T12:12:48Z
started: 2026-10-09T11:35:41Z
closed: 2026-10-09T12:12:48Z
---

## Description
Owner screenshot (2026-10-09): the AI review card shown above today's journal (summary text, topic chips, "PRÓXIMAS ACCIONES" list, "Revisar de nuevo"/"Cerrar") is taller than the space available; its top is clipped under the page header ("Página" + back/forward) and content leaves the available area. It must be bounded (max height relative to the viewport/page area) with its body in a vertical scroll container, keeping the action buttons visible.

## Acceptance Criteria
- Long reviews never overflow or get clipped; body scrolls; actions stay reachable.
- gpui test with a long review asserting bounded height / scroll container.
