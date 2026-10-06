---
id: BIT-T-0346
type: task
title: Integrate merge3 into pre-write check and watcher paths for dirty pages
status: backlog
priority: high
parent: BIT-US-0069
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, io]
estimate: 2
created: 2026-10-06T14:34:14Z
updated: 2026-10-06T15:08:58Z
---

## Description
When `PreCheck::Changed` or a watcher event hits a dirty page: run `merge3(parse(disk.bytes), parse(new), page)` where `disk.bytes` is the in-memory merge base (last bytes read/written for that file; no on-disk snapshot, ADR-017); Clean → apply ops as a non-undoable "External change" transaction, set `DiskSnapshot` to the new bytes, reschedule the write (which will then pass the hash check); Conflict → set `PageState::Conflicted` and emit `Notice::PageConflict`.

No base available (e.g. after restart, file not read/written in this session): no pending local edits → plain reload; pending local edits → `PageState::Conflicted` with a 2-way per-block diff (disk vs ours) attached to the notice (ADR-017).

## Acceptance Criteria
- Integration test: write fires after Logseq-style external edit to another block → merged file contains both changes, no prompt.
- Same-block conflict → no write, notice emitted.
- No-base case: clean page reloads; page with pending edits → conflict notice with 2-way diff, no write.

## Notes
Story BIT-US-0069. Implements BIT-SP-0005.R15, BIT-SP-0005.R4. ADR-016, ADR-017.
