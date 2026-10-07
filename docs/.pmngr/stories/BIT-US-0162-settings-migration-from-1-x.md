---
id: BIT-US-0162
type: story
title: Settings migration from 1.x
status: backlog
priority: high
parent: BIT-EP-0025
milestone: BIT-M-0009
author: mcp
labels: [v2, release, compat]
estimate: 3
created: 2026-10-07T09:19:46Z
updated: 2026-10-07T09:19:46Z
---

## Description
As an existing user, I want my 1.x settings, theme choice, layout and tokens to survive the upgrade.

## Acceptance Criteria
- Migration of app settings, theme preference (Paper kept if chosen), dock/tab layout, MCP tokens; index schema migration path tested from 1.0 DB.
- Upgrade test from a captured 1.0 profile.

## Notes
Index schema bump from semantic tables (rule 5).
