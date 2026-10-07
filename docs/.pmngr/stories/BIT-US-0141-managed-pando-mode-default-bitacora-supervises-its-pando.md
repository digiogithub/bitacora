---
id: BIT-US-0141
type: story
title: "Managed Pando mode (default): Bitacora supervises its Pando instance"
status: in_review
priority: high
parent: BIT-EP-0020
milestone: BIT-M-0007
author: mcp
labels: [v2, pando, bitacora-pando, managed]
estimate: 8
created: 2026-10-07T09:15:49Z
updated: 2026-10-07T11:03:33Z
started: 2026-10-07T10:35:51Z
---

## Description
As a user, I want Bitacora to start, configure and supervise its own Pando instance, the way git-in-track's managed mode does, so I don't have to run or configure Pando by hand. Managed is the default mode. `external` (connect to a running Pando) and `off` stay available.

## Acceptance Criteria
- A per-graph instance directory under the machine-local cache (`<cache>/pando/<graph-key>/`, mode 0700) holds:
  - a generated `.pando.toml`, rewritten on every start and merged by Pando over the user's global `~/.config/pando/.pando.toml`, so model providers and API keys are never copied;
  - the token file, `state.json`, a supervisor lock and a log with the token redacted.
  - Nothing is ever written inside the graph folder.
- The generated config contains:
  - the AG-UI adapter on loopback with `RequireToken` and empty `AllowedOrigins`;
  - Bitacora's agent profiles and personas (`bitacora-chat`, `bitacora-journal-reviewer`, `bitacora-recommender`, `bitacora-writer`) with tool allow-lists;
  - `MCPServers.bitacora` (streamable-http, loopback URL, `pando` bearer token).
  - It does not override the remembrances/KB storage, so the instance uses the user's **shared KB** (agent memory).
- The supervisor spawns `pando serve` (REST KB API, plus `--agui-port` or a separate `pando agui-serve`):
  - free loopback port, own process group, Pdeathsig on Linux and a watchdog on macOS;
  - ready check, periodic health checks, crash restart with backoff and a failed state;
  - stop on quit, adoption of orphans.
  - A minimum version is checked via `pando --version`.
- Settings show the mode and the instance status, with Restart and Open log actions.

## Notes
- Owner decision 2026-10-07: managed mode as in git-in-track; shared KB with the agent memory.
- Reference: git-in-track `internal/pando/supervisor/` (`doc.go`, `supervisor.go`, `files.go`) and `internal/agentcfg/templates/pando.toml.tmpl`, ADR-039 there.
- Pando: the REST API is served by `pando serve` (`cmd/serve.go:162`). `pando agui-serve` exposes only AG-UI.
