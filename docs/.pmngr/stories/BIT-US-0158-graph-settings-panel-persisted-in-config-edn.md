---
id: BIT-US-0158
type: story
title: Graph settings panel persisted in config.edn
status: backlog
priority: medium
parent: BIT-EP-0024
milestone: BIT-M-0009
author: mcp
labels: [v2, graph, bitacora-app, bitacora-config]
estimate: 5
created: 2026-10-07T09:19:46Z
updated: 2026-10-07T09:19:46Z
---

## Description
As a user, I want the Logseq-style graph settings panel (Nodes, Search, Forces, Export) with my choices saved like Logseq does.

## Acceptance Criteria
- Floating panel with collapsible sections: Nodes (counts, journals/orphans/builtin/excluded toggles, pause simulation), Search (label filter dims non-matching), Forces (link distance, charge strength, charge range sliders, reset), Export.
- Persist `:graph/settings` and `:graph/forcesettings` in `logseq/config.edn` via the core Op path with comment-preserving edits; unrelated bytes unchanged (test).

## Notes
Implements BIT-SP-0012.R5. Logseq refs: `components/page.cljs:577-770`, `common_config.cljc:65-66`, template `config.edn:288-296`.
