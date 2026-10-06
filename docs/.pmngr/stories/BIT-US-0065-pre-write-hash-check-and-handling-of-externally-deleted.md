---
id: BIT-US-0065
type: story
title: Pre-write hash check and handling of externally deleted files
status: done
priority: critical
parent: BIT-EP-0008
milestone: BIT-M-0003
author: mcp
labels: [core, io, bitacora-core]
estimate: 3
created: 2026-10-06T14:29:01Z
updated: 2026-10-06T20:27:13Z
closed: 2026-10-06T18:52:32Z
---

## Description
As a user who runs Logseq, git and a text editor on the same graph, I want Bitacora to check the file on disk before every write, so that it never overwrites a change it has not seen.

## Acceptance Criteria
- Before each write: `stat`; if `(len, mtime)` differ from `DiskSnapshot`, read and `blake3`-hash; equal → write, different → hand off to the external-change path (merge/conflict), no write. `DiskSnapshot` is the in-memory record of the last bytes read/written (also the merge base, ADR-017); nothing is read from the index for this check.
- Missing file + dirty page → recreate (with parent dirs) and notify; missing file + clean page → remove from model/index, no write.
- Unit tests with a temp dir: unchanged, touched-only, modified, deleted, replaced by directory (error notice).

## Notes
Implements: BIT-SP-0005.R4, BIT-SP-0005.R8, BIT-SP-0002.R17.
See [[block-editor]] §5.2 step 3, [[01-file-graph-layout]] requirement 18, [[04-editor-outliner-operations]] §5 step 7. ADR-011, ADR-017.
