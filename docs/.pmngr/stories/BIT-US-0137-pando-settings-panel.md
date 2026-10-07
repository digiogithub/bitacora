---
id: BIT-US-0137
type: story
title: Pando settings panel
status: backlog
priority: high
parent: BIT-EP-0020
milestone: BIT-M-0007
author: mcp
labels: [v2, pando, settings, bitacora-app]
estimate: 5
created: 2026-10-07T09:15:49Z
updated: 2026-10-07T09:15:49Z
---

## Description
As a user, I want a Pando settings panel reachable from a button in the top bar (AI/Pando) and from the settings window, so that I can connect Bitacora to Pando and choose which AI features to use.

## Acceptance Criteria
- Sections: Connection (enable, mode, URLs, token, allow remote, Test connection showing version/agents/model from `/info`), Features (semantic search, chat, journal review, recommendations, inline AI — each with profile picker), Graphs (per-graph consent, exclusions, MCP write grant), Activity (link to log).
- Minimum Pando version check with explanation; status chip in the panel and top bar.
- `#[gpui::test]` for form validation and state transitions.

## Notes
Implements BIT-SP-0009.R6. Design: settings window + popover components from the design system. Unlike git-in-track's search-only panel, this covers agents and consent.
