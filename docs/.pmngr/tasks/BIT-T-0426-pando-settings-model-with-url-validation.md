---
id: BIT-T-0426
type: task
title: Pando settings model with URL validation
status: backlog
priority: high
parent: BIT-US-0136
milestone: BIT-M-0007
author: mcp
labels: [v2, settings, bitacora-config]
estimate: 2
created: 2026-10-07T09:16:33Z
updated: 2026-10-07T09:16:33Z
---

## Description
Machine-local `PandoSettings` in the app settings store (not in the graph): enabled, mode, URLs, allow_remote, feature switches + profiles, per-graph consent/exclusions/write grant; validation for loopback/https.

## Acceptance Criteria
- Unit tests for validation matrix; settings file location outside graph folder (test).
