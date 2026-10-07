---
id: BIT-T-0453
type: task
title: Streaming message rendering with page and block links
status: backlog
priority: high
parent: BIT-US-0147
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, bitacora-app]
estimate: 3
created: 2026-10-07T09:19:06Z
updated: 2026-10-07T09:19:06Z
---

## Description
Chat message view rendering a markdown subset incrementally (append-only per delta), `[[Page]]`/`((uuid))` resolved to clickable links, composer with Enter/Shift+Enter, Stop button, copy message.

## Acceptance Criteria
- `#[gpui::test]`: deltas append without re-layout of previous messages; link click navigates.
