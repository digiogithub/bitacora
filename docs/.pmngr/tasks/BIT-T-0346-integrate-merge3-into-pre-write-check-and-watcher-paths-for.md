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
updated: 2026-10-06T14:34:14Z
---

## Description
When `PreCheck::Changed` or a watcher event hits a dirty page: run `merge3(parse(disk.bytes), parse(new), page)`; Clean → apply ops as a non-undoable "External change" transaction, set `DiskSnapshot` to the new bytes, reschedule the write (which will then pass the hash check); Conflict → set `PageState::Conflicted` and emit `Notice::PageConflict`.

## Acceptance Criteria
- Integration test: write fires after Logseq-style external edit to another block → merged file contains both changes, no prompt.
- Same-block conflict → no write, notice emitted.

## Notes
Story BIT-US-0069. Implements BIT-SP-0005.R15, BIT-SP-0005.R4.
