---
id: BIT-T-0265
type: task
title: End-to-end byte-exact undo test through the writer
status: backlog
priority: high
parent: BIT-US-0039
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, test]
estimate: 3
created: 2026-10-06T14:32:23Z
updated: 2026-10-06T14:32:23Z
---

## Description
`crates/bitacora-core/tests/undo_bytes_e2e.rs`: copy fixtures to a temp graph, run scripted command sequences (split, merge, indent, outdent, move, collapse, paste, cycle marker, ensure-uuid) through the command queue and real writer, flush, then undo all and flush; assert every file is byte-identical to the original (CRLF, spaces, `*` bullets, missing final newline included). Then redo all and compare with the post-sequence bytes.

## Acceptance Criteria
- Runs on Linux/macOS/Windows CI.
- Failure prints a unified diff of the first differing file.

## Notes
Story BIT-US-0039. Verifies BIT-SP-0004.R17, BIT-SP-0005.R3. Epic acceptance: "Undo/redo restores exact bytes on disk".
