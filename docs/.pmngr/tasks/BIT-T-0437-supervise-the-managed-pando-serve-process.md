---
id: BIT-T-0437
type: task
title: Supervise the managed pando serve process
status: in_progress
priority: high
parent: BIT-US-0141
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-pando]
estimate: 5
created: 2026-10-07T09:16:34Z
updated: 2026-10-07T10:35:57Z
started: 2026-10-07T10:35:57Z
---

## Description
Supervisor in `bitacora-pando` (native-only module), modelled on git-in-track's `internal/pando/supervisor`:
- locate the `pando` binary (PATH or a configured path) and check the minimum version with `pando --version`;
- take a per-instance lock, pick a free loopback port, spawn `pando serve` with the instance dir as cwd and `PANDO_CONFIG_PARENT_SEARCH=false`, in its own process group;
- Pdeathsig on Linux and a watchdog on macOS;
- ready and health checks with our token, crash backoff, a failed state and orphan adoption;
- write `state.json` atomically, keep a rotated log with the token redacted, and stop with SIGTERM then SIGKILL on quit.

## Acceptance Criteria
- Integration tests with a fake `pando` script cover readiness, a crash and restart, the failed state, and no orphan left after quit.
- Disabled inside Flatpak unless the binary is reachable.
