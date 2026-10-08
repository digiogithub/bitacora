---
id: BIT-US-0180
type: story
title: "Settings: enable Pando models for an agent model selector"
status: backlog
priority: medium
parent: BIT-EP-0026
milestone: BIT-M-0010
author: mcp
labels: [bitacora-app, bitacora-pando, ai, settings]
created: 2026-10-08T12:23:05Z
updated: 2026-10-08T12:23:05Z
---

## Description
Pando's model configuration is unreachable. Settings list the models available from Pando and let the user enable a subset; the Agent panel shows a selector with the enabled models (default first) and uses the chosen one per conversation.

## Acceptance Criteria
- Models listed through existing generic Pando APIs/config (no Bitacora-specific Pando change).
- Enabled models persisted in app settings.
- Agent selector sends the chosen model with the run.
