---
id: BIT-T-0116
type: task
title: "Settings > Agents page: tokens, scopes, allowed origins and config snippet"
status: done
priority: high
parent: BIT-US-0016
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, mcp, ui, settings]
estimate: 3
created: 2026-10-06T14:29:54Z
updated: 2026-10-06T21:53:47Z
closed: 2026-10-06T21:53:47Z
---

## Description
GPUI Kit settings page in `crates/bitacora-app/src/settings/agents.rs`: server status, port, `allow_writes` / `allow_deletes` / `confirm_writes` toggles (default off), allowed origins list, protected namespaces, token table (name, scopes, created, revoke), "New token" dialog showing the secret once, and "Copy config snippet" producing `{"type":"http","url":"http://127.0.0.1:12316/mcp","headers":{"Authorization":"Bearer …"}}` for Claude Desktop/Code. Changes persist via `bitacora-config` and hot-reload `AuthState`.

## Acceptance Criteria
- `#[gpui::test]` for token create/revoke view logic.
- Revoking a token rejects its next request without restart.

## Notes
Story BIT-US-0016. Implements BIT-SP-0007.R6. See [[mcp-server]] §8.
