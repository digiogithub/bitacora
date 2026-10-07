---
id: BIT-US-0147
type: story
title: Chat view with streaming answers and threads
status: in_progress
priority: high
parent: BIT-EP-0022
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, agui, bitacora-app, bitacora-pando]
estimate: 8
created: 2026-10-07T09:18:16Z
updated: 2026-10-07T12:17:11Z
started: 2026-10-07T12:17:11Z
---

## Description
As a user, I want to chat with Pando about my notes in the Agent tab, with answers streaming in and conversations kept as threads.

## Acceptance Criteria
- Chat entity drives AG-UI runs through `bitacora-pando` (profile `bitacora-chat`), events delivered to GPUI via channels.
- Streaming text rendered incrementally (markdown subset, `[[Page]]` and `((block))` refs become links); composer with Enter/Shift+Enter; Stop cancels the run.
- Threads list (from `/threads`), resume (re-attach + `MESSAGES_SNAPSHOT`), new, delete.

## Notes
Implements BIT-SP-0011.R3. Requires consent for feature "chat".
