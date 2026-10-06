---
id: BIT-T-0323
type: task
title: Handle externally deleted files for dirty and clean pages
status: backlog
priority: high
parent: BIT-US-0065
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-app, io]
estimate: 2
created: 2026-10-06T14:33:18Z
updated: 2026-10-06T14:33:18Z
---

## Description
On `PreCheck::Missing` with a dirty page: create parent dirs, write via `atomic_write`, emit `Notice::Restored { path }`. On watcher `Removed` for a clean page: submit `External` command removing the page from the model and index (no disk write; journals become virtual if today's). `NotAFile` → error notice, page stays dirty.

## Acceptance Criteria
- Integration test: delete file of dirty page → file recreated + notice.
- Delete file of clean page → page removed, nothing written, `logseq/.recycle` untouched.

## Notes
Story BIT-US-0065. Implements BIT-SP-0005.R8. [[01-file-graph-layout]] §10.3.
