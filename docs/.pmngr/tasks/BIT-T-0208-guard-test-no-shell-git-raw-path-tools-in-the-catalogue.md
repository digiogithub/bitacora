---
id: BIT-T-0208
type: task
title: "Guard test: no shell/git/raw-path tools in the catalogue"
status: done
priority: medium
parent: BIT-US-0021
milestone: BIT-M-0003
author: mcp
labels: [bitacora-mcp, security, testing]
estimate: 1
created: 2026-10-06T14:31:00Z
updated: 2026-10-06T19:40:50Z
closed: 2026-10-06T19:40:50Z
---

## Description
`crates/bitacora-mcp/tests/catalogue_guard.rs`: call `tools/list` with a full-scope token and assert no tool name matches `exec|shell|command|git_(?!sync_)` and no input schema property named `path`, `file`, `cmd`, `args` on write tools; annotation checks (`readOnlyHint` on all read tools, `destructiveHint` on delete tools).

## Acceptance Criteria
- Test fails if a forbidden tool/argument is added.

## Notes
Story BIT-US-0021. Verifies BIT-SP-0007.R15, BIT-SP-0007.R16.
