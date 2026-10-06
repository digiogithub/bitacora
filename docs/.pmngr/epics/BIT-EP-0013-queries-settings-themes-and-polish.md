---
id: BIT-EP-0013
type: epic
title: Queries, settings, themes and polish
status: backlog
priority: medium
milestone: BIT-M-0005
author: mcp
labels: [ui, query]
created: 2026-10-06T14:21:13Z
updated: 2026-10-06T14:21:13Z
---

## Description
Simple query DSL (`{{query ...}}`) and the supported Datalog subset compiled to SQL with result rendering, embeds (`{{embed}}`), slash and `<` commands, templates, drag & drop of blocks, settings UI (graph, sync, MCP, appearance, keymap), themes (light/dark, `custom.css` subset where feasible), i18n, performance and accessibility passes.

## Acceptance Criteria
- Query cases from [[sqlite-index-schema]] §7–8 render the same results as Logseq on fixtures.
- Settings persist and are applied without restart where possible.

## Notes
See [[sqlite-index-schema]], [[block-editor]] §9 (post-MVP list).
