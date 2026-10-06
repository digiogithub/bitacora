---
id: BIT-EP-0008
type: epic
title: Safe write path and external change handling
status: backlog
priority: critical
milestone: BIT-M-0003
author: mcp
labels: [core, io]
created: 2026-10-06T14:21:13Z
updated: 2026-10-06T15:12:33Z
---

## Description
Single-writer command queue, debounced atomic writes (temp + fsync + rename), pre-write hash check, `logseq/bak` backups in Logseq layout, `bitacora-watch` (notify + debouncer, echo suppression), and block-level merge of external edits into open pages (e.g. Logseq or a text editor editing the same graph).

## Acceptance Criteria
- Killing the process mid-write never leaves a truncated file.
- External edit to an open page is merged without losing either side's content; conflicts surface in the UI.
- Our own writes never trigger a reparse loop.

## Notes
ADR-011. See [[block-editor]] §5–6. ADR-016: the external-edit merge uses the shared `bitacora-merge` crate. ADR-017: merge base kept in memory only (no `file_snapshots`); after restart, reload if clean, else 2-way diff in the conflict notice.
