---
id: BIT-US-0125
type: story
title: Right panel with Context and Agent tabs
status: done
priority: medium
parent: BIT-EP-0017
milestone: BIT-M-0006
author: mcp
labels: [v2, ui, bitacora-app]
estimate: 5
created: 2026-10-07T09:13:09Z
updated: 2026-10-07T11:41:51Z
closed: 2026-10-07T11:41:51Z
---

## Description
As a user, I want the 360px right panel of the design with Context (linked refs, page properties, related, local graph slot) and Agent tabs (MCP activity now; Pando chat later), toggled from the top bar.

## Acceptance Criteria
- Existing right-sidebar content (refs, agent activity) reachable from the new tabs; underline tab indicator; panel state persisted.
- Slots ready for Pando chat, recommendations, related blocks and local graph.

## Notes
Current: `views/agent_activity.rs`, right dock in `views/workspace.rs`.
