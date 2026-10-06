---
id: BIT-T-0227
type: task
title: Wire Tab, Shift+Tab, Alt+Shift+Up/Down and logical-outdent setting; golden tests
status: done
priority: high
parent: BIT-US-0034
milestone: BIT-M-0003
author: mcp
labels: [bitacora-app, bitacora-core, test]
estimate: 2
created: 2026-10-06T14:31:33Z
updated: 2026-10-06T21:12:28Z
closed: 2026-10-06T21:12:28Z
---

## Description
Bind actions in `BlockEditor` and `BlockSelection` contexts; keep edit mode/caret on the moved block; read `:editor/logical-outdenting?` from `config.edn` via `bitacora-config`. Add `crates/bitacora-core/tests/cmd_indent_move.rs` golden cases (tabs, 2 spaces, 4 spaces, CRLF, blocks with continuation lines) with undo byte-exact assertions.

## Acceptance Criteria
- ≥ 12 golden cases; only indentation prefixes differ for clean blocks.
- `#[gpui::test]`: caret stays in the moved block after `Tab`.

## Notes
Story BIT-US-0034. Implements BIT-SP-0004.R9, BIT-SP-0004.R10.
