---
id: BIT-T-0272
type: task
title: Verify portals, git/SSH and MCP inside the Flatpak sandbox
status: backlog
priority: low
parent: BIT-US-0112
milestone: BIT-M-0005
author: mcp
labels: [packaging, linux, bitacora-app]
estimate: 2
created: 2026-10-06T14:32:50Z
updated: 2026-10-06T15:15:56Z
---

## Description
Inside the Flatpak build, verify and fix: opening a graph via `rfd` file-chooser portal (document portal paths vs real paths; persist access across restarts), opening links via OpenURI portal (`open` crate), sync fetch/push over SSH with the host agent and over HTTPS — with the gix-only backend when no `git` is visible in the sandbox (expected default, git is not bundled per ADR-020) and with system git if the runtime provides one — keyring via Secret Service, MCP reachable at `127.0.0.1:12316` from a host client, single-instance IPC across launches, and auto-update disabled when `FLATPAK_ID` is set. Document limitations in `docs/design/` (append to the auto-update/packaging page).

## Acceptance Criteria
- Each item in the description has a recorded pass/fail; failures either fixed or documented with a workaround.

## Notes
- [[crate-stack]] §4.1 (`rfd` portal support, `keyring` Secret Service, Risk R9); [[mcp-server]] §2; ADR-007, ADR-020.
