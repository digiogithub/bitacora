---
id: BIT-US-0152
type: story
title: "AI recommendations: related pages, links, tags and next actions"
status: in_review
priority: medium
parent: BIT-EP-0023
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, bitacora-app, bitacora-pando]
estimate: 8
created: 2026-10-07T09:18:16Z
updated: 2026-10-07T12:54:55Z
started: 2026-10-07T12:54:55Z
---

## Description
As a user, I want suggestions while I work: related pages, missing links, tags and next actions for the current page, that I can accept with one click.

## Acceptance Criteria
- Recommender runs on demand and (optionally) debounced on page open; profile `bitacora-recommender` uses semantic search + MCP reads; structured JSON suggestions.
- Context tab shows amber chips/cards; Accept applies one undoable Op (link insertion, `tags::` edit, new TODO block); Dismiss remembered per content hash.
- Respects consent, exclusions and the feature switch.

## Notes
Implements BIT-SP-0011.R5. Depends on semantic search and approval infrastructure.
