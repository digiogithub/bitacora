---
id: BIT-T-0163
type: task
title: Load user keymap overrides with validation
status: backlog
priority: medium
parent: BIT-US-0031
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, keymap]
estimate: 2
created: 2026-10-06T14:30:44Z
updated: 2026-10-06T14:30:44Z
---

## Description
Read `<config_dir>/bitacora/keymap.json` (list of `{ context, bindings: { "keys": "action" } }`) at startup and on change; merge over defaults; unknown actions/contexts or unparsable keystrokes are skipped and reported in a single warning notice + log.

## Acceptance Criteria
- Overriding `editor::CycleTodo` to `ctrl-t` works in `BlockEditor`.
- Invalid entry keeps defaults and lists the entry in the warning.
- Missing file = defaults, no warning.

## Notes
Story BIT-US-0031. Implements BIT-SP-0004.R21. Settings infra may come from `bitacora-config`.
