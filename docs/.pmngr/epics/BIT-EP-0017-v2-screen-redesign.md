---
id: BIT-EP-0017
type: epic
title: v2 screen redesign
status: backlog
priority: high
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, design-system, bitacora-app]
created: 2026-10-07T09:10:26Z
updated: 2026-10-07T09:10:26Z
---

## Description
Rebuild the screens after the design system mockups (`mockups/Main.dc.html`, `Claro.dc.html`, `Tareas.dc.html`; `docs/layout.md`, `docs/componentes.md`, `docs/interaccion.md`): top bar content and tabs, left sidebar (nav, calendar, favorites/recents, graph + MCP footer), outline and journal styling, right panel (Context / Agent tabs), Tasks view, popovers and modals (PDF, settings, palette, conflicts), narrow-window behaviour.

## Acceptance Criteria
- Every screen matches the mockups in dark and light (manual screenshot review recorded).
- All existing behaviour, shortcuts and `#[gpui::test]` suites unchanged; file round-trip unaffected.

## Notes
- Plan [[bitacora-v2-plan]]. Depends on the design system foundation and frameless epics.
- AI surfaces (Agent tab chat, inline AI, ⌘J) are delivered by the M7 epics; here only their containers.
