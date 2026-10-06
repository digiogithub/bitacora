---
id: BIT-T-0326
type: task
title: "Write failure handling: retry with backoff, persistent notice, unwritten-files report"
status: done
priority: high
parent: BIT-US-0066
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-app, io]
estimate: 3
created: 2026-10-06T14:33:19Z
updated: 2026-10-06T18:52:32Z
closed: 2026-10-06T18:52:32Z
---

## Description
On `WriteError` keep the page dirty, schedule retries (1 s, 2 s, 4 s … cap 60 s), try saving new bytes to `logseq/bak` once, and emit `Notice::WriteFailed { path, error }` (persistent until success). Batch failures produce `Notice::UnwrittenFiles(Vec<RelPath>)`. App: notice UI (GPUI Kit toast/banner) and a quit-time confirmation listing unsaved pages.

## Acceptance Criteria
- Test with read-only file (Unix) / locked file (Windows): notice shown, retry succeeds after permissions restored, notice cleared.
- Multi-file batch with one failure lists only the failed file.

## Notes
Story BIT-US-0066. Implements BIT-SP-0005.R10. [[block-editor]] §5.3.
