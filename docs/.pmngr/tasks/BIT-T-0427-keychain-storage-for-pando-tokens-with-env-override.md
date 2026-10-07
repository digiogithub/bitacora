---
id: BIT-T-0427
type: task
title: Keychain storage for Pando tokens with env override
status: backlog
priority: high
parent: BIT-US-0136
milestone: BIT-M-0007
author: mcp
labels: [v2, security]
estimate: 1
created: 2026-10-07T09:16:33Z
updated: 2026-10-07T09:16:33Z
---

## Description
Store REST and AG-UI tokens with the existing keyring backend; `BITACORA_PANDO_TOKEN`/`BITACORA_PANDO_AGUI_TOKEN` env overrides; `Secret` wrapper with redacted Debug.

## Acceptance Criteria
- Test: trace logs of a search contain no token; Debug output redacted.
