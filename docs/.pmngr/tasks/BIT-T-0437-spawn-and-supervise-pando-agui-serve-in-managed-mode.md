---
id: BIT-T-0437
type: task
title: Spawn and supervise pando agui-serve in managed mode
status: backlog
priority: low
parent: BIT-US-0141
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-pando]
estimate: 3
created: 2026-10-07T09:16:34Z
updated: 2026-10-07T09:16:34Z
---

## Description
Locate `pando` on PATH or configured path, spawn REST + AG-UI servers on loopback with a generated token, restart with backoff, stop on quit; disabled inside Flatpak unless the binary is reachable.

## Acceptance Criteria
- Integration test with a fake `pando` script; process never orphaned on quit.
