---
id: BIT-T-0325
type: task
title: Invoke backups on destructive writes, keep-mine overwrites and take-disk replacements
status: done
priority: high
parent: BIT-US-0066
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, io]
estimate: 2
created: 2026-10-06T14:33:18Z
updated: 2026-10-06T18:52:31Z
closed: 2026-10-06T18:52:31Z
---

## Description
Call `backup` from the writer when `removes_text(disk.bytes, new_bytes)`; from conflict resolution "Keep mine" (backup disk bytes) and "Take disk" (backup our serialized bytes); and when an external version replaces unsaved content. Make backups configurable (`settings.backups.enabled`, default on). Ensure `logseq/bak` is in watcher and indexer ignore lists.

## Acceptance Criteria
- Integration tests for each trigger.
- No backup on append-only edits.

## Notes
Story BIT-US-0066. Implements BIT-SP-0005.R9.
