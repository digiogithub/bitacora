---
id: BIT-T-0418
type: task
title: Prefix-scoped API tokens
status: backlog
priority: medium
parent: BIT-US-0133
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, security]
estimate: 2
created: 2026-10-07T09:15:16Z
updated: 2026-10-07T09:15:16Z
---

## Description
Configurable extra tokens restricted to KB routes and a `path_prefix` (e.g. `bitacora/`); middleware enforces scope on search/list/upsert/delete.

## Acceptance Criteria
- Go tests: out-of-scope document → 403; search results filtered to scope.
