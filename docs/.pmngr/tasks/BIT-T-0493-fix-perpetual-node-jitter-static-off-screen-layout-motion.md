---
id: BIT-T-0493
type: task
title: "Fix perpetual node jitter: static off-screen layout, motion only while dragging"
status: done
priority: high
parent: BIT-US-0157
author: mcp
labels: [v2, graph, bitacora-app, bug]
created: 2026-10-07T15:58:15Z
updated: 2026-10-07T15:59:03Z
closed: 2026-10-07T15:59:03Z
---

Owner bug (2026-10-07): graph nodes trembled non-stop. Root cause: the sim worker counted Pin messages (one per pointer move) against a single Unpin, so alpha_target stayed 0.3 after any drag and the layout never cooled. Fix: worker tracks the held-node set, quick cool-down + exact freeze on release; layouts (initial, refresh, focus, forces, local graph) are computed off-screen and shown static; refresh pins surviving nodes; frames only while dragging. Applies to global and local graph.
