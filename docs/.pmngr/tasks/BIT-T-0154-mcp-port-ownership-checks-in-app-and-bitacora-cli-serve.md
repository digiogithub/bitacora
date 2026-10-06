---
id: BIT-T-0154
type: task
title: MCP port ownership checks in app and bitacora-cli serve
status: done
priority: high
parent: BIT-US-0086
milestone: BIT-M-0005
author: mcp
labels: [release, mcp, bitacora-app, bitacora-cli, bitacora-mcp]
estimate: 2
created: 2026-10-06T14:30:21Z
updated: 2026-10-06T20:18:19Z
closed: 2026-10-06T20:18:19Z
---

## Description
- `bitacora-cli serve`: acquire `InstanceLock(Headless)`; if a desktop instance holds it, print "Bitacora desktop is running and already serves MCP on 127.0.0.1:<port>" and exit with code 3.
- Desktop app: if the lock is held by a headless server, show a dialog offering to stop it (send IPC `shutdown` if the CLI implements the same listener) or start without MCP.
- On MCP bind failure (`AddrInUse`), probe `GET http://127.0.0.1:<port>/health`; distinguish "another Bitacora" vs "foreign process", surface it in the status bar MCP slot and settings, and never pick another port silently.
- Record the bound port in `instance.json`.

## Acceptance Criteria
- Tests with a real listener occupying the port: app/CLI report the correct reason; exit codes documented in `bitacora-cli --help`.
- Status bar shows MCP state `Running(port)` / `Disabled(reason)`.

## Notes
- [[mcp-server]] §2 (port 12316 default, fail visibly, `/health`). AGENTS.md §3 rule 7. Related: BIT-EP-0010, BIT-SP-0007.
