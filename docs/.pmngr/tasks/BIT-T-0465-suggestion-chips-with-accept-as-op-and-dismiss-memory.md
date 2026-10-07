---
id: BIT-T-0465
type: task
title: Suggestion chips with accept-as-Op and dismiss memory
status: backlog
priority: medium
parent: BIT-US-0152
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, bitacora-app]
estimate: 3
created: 2026-10-07T09:19:06Z
updated: 2026-10-07T09:19:06Z
---

## Description
Context tab section with amber chips/cards; Accept applies one undoable Op (link wrap, `tags::` update, new TODO block); Dismiss stored per content hash so it is not re-offered.

## Acceptance Criteria
- `#[gpui::test]`: accept → one undo step; dismissed suggestion hidden after re-run.
