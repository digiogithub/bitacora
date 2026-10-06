---
id: BIT-US-0010
type: story
title: Index diagnostics, integrity check and CLI reindex/doctor
status: backlog
priority: medium
parent: BIT-EP-0005
milestone: BIT-M-0002
author: mcp
labels: [index, bitacora-index, bitacora-cli]
estimate: 3
created: 2026-10-06T14:25:24Z
updated: 2026-10-06T14:25:24Z
---

## Description
As a user or support engineer, I want to see why a page is missing or duplicated and to force a clean rebuild from the CLI, so that index problems are diagnosable and always recoverable.

## Acceptance Criteria
- `bitacora-cli reindex --graph <path>` deletes and rebuilds the index, printing counts and duration.
- `bitacora-cli doctor --graph <path>` runs `quick_check`, `foreign_key_check`, FTS `integrity-check` and lists diagnostics grouped by kind.
- Diagnostics API returns rows per file and per kind for the UI.

## Notes
Implements: BIT-SP-0003.R1, BIT-SP-0003.R11, BIT-SP-0003.R16. See [[sqlite-index-schema]] §3 (`diagnostics`), AGENTS.md §2 (`bitacora-cli`).
