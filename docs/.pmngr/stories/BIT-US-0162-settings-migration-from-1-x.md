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
updated: 2026-10-07T09:55:09Z
---

## Description
As an existing user, I want my 1.x settings, layout, last graph and tokens to survive the upgrade.

## Acceptance Criteria
- Migrate app settings, theme preference, dock/tab layout, recent graphs and MCP tokens.
  - System/Light/Dark is kept; a Paper or custom theme selection maps to the Bitacora theme for that mode.
- Test the index schema migration path from a 1.0 DB.
- Upgrade test from a captured 1.0 profile.

## Notes
Paper theme removed (owner decision 2026-10-07).
