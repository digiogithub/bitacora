---
id: BIT-US-0066
type: story
title: logseq/bak backups and write-failure recovery
status: done
priority: high
parent: BIT-EP-0008
milestone: BIT-M-0003
author: mcp
labels: [core, io, bitacora-core, bitacora-app]
estimate: 5
created: 2026-10-06T14:29:01Z
updated: 2026-10-06T18:52:32Z
closed: 2026-10-06T18:52:32Z
---

## Description
As a Logseq user, I want previous versions of a page saved in `logseq/bak/` (where Logseq puts them) before destructive overwrites, and clear notices when a file cannot be saved, so that I can always recover content.

## Acceptance Criteria
- Backup path `logseq/bak/<rel dir>/<stem>/<ISO ts, ':'→'_'>.Desktop.<ext>`; keep newest 6 per directory; atomic writes; never indexed or watched.
- Backup triggers: write that removes text vs previous disk content; "Keep mine" overwrite of an external version; external version replacing unsaved content.
- Write failure: page stays dirty, retry with backoff (1 s → 60 s cap), new content saved to bak when possible, persistent notice naming the file and error; warning on quit with unsaved pages.
- Multi-file transactions report the list of files not written.

## Notes
Implements: BIT-SP-0005.R9, BIT-SP-0005.R10.
See [[01-file-graph-layout]] §11 and requirement 19, [[block-editor]] §5.2 step 7 and §5.3, [[04-editor-outliner-operations]] §5 step 10. ADR-011.
