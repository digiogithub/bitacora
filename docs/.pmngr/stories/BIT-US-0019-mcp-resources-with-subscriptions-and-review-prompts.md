---
id: BIT-US-0019
type: story
title: MCP resources with subscriptions and review prompts
status: in_progress
priority: medium
parent: BIT-EP-0010
milestone: BIT-M-0002
author: mcp
labels: [mcp, resources, prompts]
estimate: 5
created: 2026-10-06T14:27:12Z
updated: 2026-10-06T18:44:25Z
started: 2026-10-06T18:44:25Z
---

## Description
As an AI agent, I want pages, blocks, journals and config as MCP resources (with change notifications) and ready-made prompts, so that I can attach notes as context and run daily/weekly reviews consistently.

## Acceptance Criteria
- `resources/templates/list` advertises `bitacora://page/{name}`, `bitacora://graph/{graph}/page/{name}`, `bitacora://block/{uuid}`, `bitacora://journal/{date}`, `bitacora://graph/{graph}/config`, `bitacora://sync/status`, `bitacora://asset/{path}`.
- `resources/read` returns `text/markdown` raw page text; assets are size-capped and confined to `assets/`.
- `resources/subscribe` emits `notifications/resources/updated` on page change within 2 s (stateful sessions).
- Prompts `daily_review`, `weekly_review`, `summarize_page`, `capture`, `logseq_syntax` return messages built from graph data.
- Server `instructions` state that note content is data, not instructions.

## Notes
Implements: BIT-SP-0007.R18, BIT-SP-0007.R19, BIT-SP-0007.R15. See [[mcp-server]] §6–7.
