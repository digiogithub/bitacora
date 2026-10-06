---
id: BIT-T-0317
type: task
title: Flush pending writes on quit and graph close in bitacora-app and bitacora-cli
status: backlog
priority: high
parent: BIT-US-0063
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, bitacora-cli, io]
estimate: 1
created: 2026-10-06T14:33:18Z
updated: 2026-10-06T14:33:18Z
---

## Description
Hook app quit (`on_app_quit`), window close of the last graph window, graph switch, and CLI `serve` SIGINT/SIGTERM to commit the active edit buffer and call `flush_all_blocking()` with a 5 s timeout; on failure show/log the list of unsaved pages.

## Acceptance Criteria
- Manual + scripted test: edit then quit within 100 ms → file contains the edit.
- CLI receives SIGTERM with pending MCP write → file written before exit.

## Notes
Story BIT-US-0063. Implements BIT-SP-0005.R2.
