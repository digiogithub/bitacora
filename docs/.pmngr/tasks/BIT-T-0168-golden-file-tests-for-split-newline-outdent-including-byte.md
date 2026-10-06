---
id: BIT-T-0168
type: task
title: Golden file tests for split/newline/outdent including byte-exact undo
status: done
priority: high
parent: BIT-US-0032
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, test]
estimate: 2
created: 2026-10-06T14:30:44Z
updated: 2026-10-06T20:21:40Z
closed: 2026-10-06T20:21:40Z
---

## Description
Add `crates/bitacora-core/tests/cmd_split.rs` with fixture pairs (`before.md`, command, `after.md`) covering tab and 2-space indentation, CRLF, blocks with properties and children, collapsed parent. Each case also asserts undo returns the original bytes.

## Acceptance Criteria
- ≥ 12 golden cases; untouched lines are byte-identical in `after.md`.
- Undo assertion in every case.

## Notes
Story BIT-US-0032. Verifies BIT-SP-0004.R6, BIT-SP-0004.R17.
