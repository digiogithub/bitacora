---
id: BIT-T-0431
type: task
title: "ContentPolicy: private pages and exclusions"
status: backlog
priority: high
parent: BIT-US-0138
milestone: BIT-M-0007
author: mcp
labels: [v2, privacy, bitacora-pando]
estimate: 2
created: 2026-10-07T09:16:34Z
updated: 2026-10-07T09:16:34Z
---

## Description
Shared policy evaluating `private:: true` (configurable property), excluded pages/namespaces/tags against index data; used by semantic sync, MCP readers for the `pando` token and context attachment.

## Acceptance Criteria
- Unit tests over a fixture graph; same verdict from all three call sites.
