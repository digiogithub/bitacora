---
id: BIT-T-0201
type: task
title: Validate agent block content and blocks[] trees with the Markdown parser
status: done
priority: high
parent: BIT-US-0020
milestone: BIT-M-0003
author: mcp
labels: [bitacora-mcp, bitacora-markdown, write]
estimate: 2
created: 2026-10-06T14:31:00Z
updated: 2026-10-06T19:40:50Z
closed: 2026-10-06T19:40:50Z
---

## Description
`crates/bitacora-mcp/src/write/validate.rs`: `validate_single_block(content) -> Result<ParsedBlock, ToolError>` parsing with `bitacora_markdown` (as if prefixed by `- `) and rejecting >1 block, reserved props `id`/`collapsed` unless explicitly allowed; `validate_tree(blocks: &[BatchBlock])` recursively with a 200-block cap. Normalize line endings to the target file's style later in core.

## Acceptance Criteria
- Tests: `"first\n- second"` → `INVALID_CONTENT`; `"x\nid:: 1234"` → `INVALID_CONTENT`; multi-line with property accepted; 201 blocks rejected.

## Notes
Story BIT-US-0020. Implements BIT-SP-0007.R13. See [[02-markdown-block-syntax]].
