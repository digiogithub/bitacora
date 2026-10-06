---
id: BIT-US-0063
type: story
title: Debounced write queue with snapshot rebase after each write
status: done
priority: critical
parent: BIT-EP-0008
milestone: BIT-M-0003
author: mcp
labels: [core, io, bitacora-core]
estimate: 5
created: 2026-10-06T14:29:01Z
updated: 2026-10-06T18:52:32Z
closed: 2026-10-06T18:52:32Z
---

## Description
As a user, I want my edits saved to disk within about half a second without the UI ever stalling, so that Logseq, git or another editor sees my changes quickly and I never lose work.

## Acceptance Criteria
- `WriteQueue { pending: BTreeMap<PageKey, Instant>, debounce: 400 ms, max_delay: 2 s }` schedules dirty pages; deduplicated per page.
- Pages touched by one transaction flush in the same batch; `RenameFile` runs before the content write.
- I/O runs off the UI thread; the UI never blocks on writes.
- After a successful write: `DiskSnapshot` updated (bytes, blake3, mtime, len), origins rebased to new offsets, page clean; edits during an in-flight write keep the page dirty. `DiskSnapshot.bytes` is the in-memory merge base for external edits and is never persisted (no `file_snapshots`, ADR-017).
- Synchronous flush on quit / graph close.
- Tests with a fake clock for debounce/max-delay.

## Notes
Implements: BIT-SP-0005.R2, BIT-SP-0005.R7.
See [[block-editor]] §5.2–5.3, [[04-editor-outliner-operations]] §5 (Logseq 1 s batch). ADR-011, ADR-017.
