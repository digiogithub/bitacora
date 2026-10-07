---
id: BIT-US-0141
type: story
title: Optional managed Pando mode
status: backlog
priority: low
parent: BIT-EP-0020
milestone: BIT-M-0007
author: mcp
labels: [v2, pando, bitacora-pando]
estimate: 3
created: 2026-10-07T09:15:49Z
updated: 2026-10-07T09:15:49Z
---

## Description
As a user who does not run Pando as a service, I want Bitacora to start and supervise `pando agui-serve` (and the REST server) for me, like git-in-track's managed mode.

## Acceptance Criteria
- Mode `managed`: locate the `pando` binary, spawn on loopback with generated token, restart with backoff, stop on quit; `external` mode unchanged.
- Decision whether to ship in 2.0 recorded (owner open question).

## Notes
Reference: git-in-track ADR-039 modes auto/managed/external/off.
