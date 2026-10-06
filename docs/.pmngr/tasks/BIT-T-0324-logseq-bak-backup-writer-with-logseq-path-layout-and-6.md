---
id: BIT-T-0324
type: task
title: logseq/bak backup writer with Logseq path layout and 6-version retention
status: backlog
priority: high
parent: BIT-US-0066
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, io]
estimate: 2
created: 2026-10-06T14:33:18Z
updated: 2026-10-06T14:33:18Z
---

## Description
`crates/bitacora-core/src/writer/backup.rs`: `backup(graph_root, rel_path, bytes, now)` → `logseq/bak/<rel dir>/<stem>/<YYYY-MM-DDTHH_MM_SS.mmmZ>.Desktop.<ext>` written with `atomic_write`; then keep the 6 newest files in that directory (by name order, as Logseq `truncate-old-versioned-files!`). `removes_text(old, new) -> bool` uses a line/char diff (`similar` crate) to detect deletions.

## Acceptance Criteria
- Path format test equals Logseq example `logseq/bak/pages/foo/2025-11-14T09_30_12.345Z.Desktop.md`.
- Retention test with 7 backups.
- `removes_text` true for deletion, false for pure append.

## Notes
Story BIT-US-0066. Implements BIT-SP-0005.R9. Logseq `backup_file.cljs:27-51`; [[01-file-graph-layout]] §11.
