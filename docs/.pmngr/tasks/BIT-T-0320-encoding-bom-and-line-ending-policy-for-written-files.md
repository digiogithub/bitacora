---
id: BIT-T-0320
type: task
title: Encoding, BOM and line-ending policy for written files
status: backlog
priority: high
parent: BIT-US-0064
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, io]
estimate: 2
created: 2026-10-06T14:33:18Z
updated: 2026-10-06T14:33:18Z
---

## Description
Ensure the writer path honours `FileStyle`: new files UTF-8, LF, no BOM, tab indent; edited files keep detected line ending, leading BOM (stored in `Page` as `has_bom`, stripped from parsed text only) and final-newline state. Pasted/typed `\r\n` in block text normalised to the file's line ending on render.

## Acceptance Criteria
- Golden tests for BIT-SP-0005.R6 scenarios (CRLF append, new page bytes, BOM kept).
- Byte-level assertion that a new page has no `EF BB BF` and no `\r`.

## Notes
Story BIT-US-0064. Implements BIT-SP-0005.R6. [[01-file-graph-layout]] §8 and requirement 17.
