---
id: BIT-T-0457
type: task
title: "Frontend tool set: propose_edit, open_page, get_selection"
status: done
priority: high
parent: BIT-US-0149
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, bitacora-pando, bitacora-core]
estimate: 3
created: 2026-10-07T09:19:06Z
updated: 2026-10-07T11:50:52Z
closed: 2026-10-07T11:50:52Z
---

## Description
JSON schemas and handlers; `propose_edit` takes a constrained op list (insert/update/move/delete block, set property) mapped to core `Op`s and validated against current block state (stale proposals rejected).

## Acceptance Criteria
- Unit tests for schema validation and op mapping; stale proposal rejected.
