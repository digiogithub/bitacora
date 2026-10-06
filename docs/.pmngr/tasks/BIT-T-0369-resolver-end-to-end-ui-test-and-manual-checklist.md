---
id: BIT-T-0369
type: task
title: Resolver end-to-end UI test and manual checklist
status: backlog
priority: medium
parent: BIT-US-0054
milestone: BIT-M-0004
author: mcp
labels: [bitacora-app, merge, ui, testing]
estimate: 2
created: 2026-10-06T14:34:50Z
updated: 2026-10-06T14:34:50Z
---

## Description
`crates/bitacora-app/tests/conflict_resolver.rs` (`#[gpui::test]` with headless core + temp repos): create a 3-conflict merge state, resolve via view actions, assert files written, resolve commit created and state cleared; restart simulation restores partially resolved state. Add manual checklist to `docs/design/block-editor.md` merge UI section (keyboard flow, large pages, dark theme).

## Acceptance Criteria
- Test passes in CI; checklist committed.

## Notes
Story BIT-US-0054. Verifies BIT-SP-0006.R15, BIT-SP-0006.R16.
