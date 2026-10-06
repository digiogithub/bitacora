---
id: BIT-US-0052
type: story
title: "File-level merge cases: special files, add/add journals, renames, delete/modify"
status: backlog
priority: high
parent: BIT-EP-0012
milestone: BIT-M-0004
author: mcp
labels: [merge, sync, files]
estimate: 8
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T15:10:56Z
---

## Description
As a user, I want config, CSS, whiteboards, assets, concurrently created journals, renamed and deleted pages to merge sensibly, so that sync never corrupts or silently drops files.

## Acceptance Criteria
- `policy_for(path)` dispatch: Markdown / Config (EDN-aware map merge preserving comments) / TextLine (diff3) / Whole-file keep-both (whiteboards) / Binary (assets conflict copies) / Ignore.
- Add/add pages: empty base + union; template-only/`-`/empty side treated as unchanged.
- Renames: apply edits to the renamed path; rename/rename conflict with alias suggestion; post-merge `[[Old]]` → `[[New]]` fix-up pass; case-only renames via temp name.
- File delete vs modify → `file_delete_vs_modify` conflict (default restore).
- Non-UTF-8 `.md` → binary policy; BOM preserved.

## Notes
Implements: BIT-SP-0006.R17, BIT-SP-0006.R18, BIT-SP-0006.R19. See [[git-sync-merge]] §5, [[01-file-graph-layout]], [[05-git-and-apis]] §1.5 (journal template guard).
ADR-016: add/add page merge and line diff3 live in `bitacora-merge`; path policies (incl. `config.edn` via `bitacora-config`), renames, delete/modify and asset copies stay in `bitacora-sync`.
