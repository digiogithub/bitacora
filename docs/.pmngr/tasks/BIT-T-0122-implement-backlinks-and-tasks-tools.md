---
id: BIT-T-0122
type: task
title: Implement backlinks and tasks tools
status: done
priority: medium
parent: BIT-US-0018
milestone: BIT-M-0002
author: mcp
labels: [bitacora-mcp, tools, read]
estimate: 2
created: 2026-10-06T14:29:55Z
updated: 2026-10-06T19:01:03Z
started: 2026-10-06T18:44:25Z
closed: 2026-10-06T19:01:03Z
---

## Description
`crates/bitacora-mcp/src/tools/refs_tasks.rs`: `backlinks {name|uuid, include_unlinked=false}` using index refs/path-refs, grouped by page with breadcrumbs (unlinked via FTS on page name + aliases); `tasks {status?[], page?, scheduled_before?, deadline_before?, priority?, limit}` over the index task columns.

## Acceptance Criteria
- Fixture tests: alias-linked references counted, unlinked excludes linked, status set filter, date bounds.

## Notes
Story BIT-US-0018. Implements BIT-SP-0007.R11. Mirrors `Editor.getPageLinkedReferences`.
