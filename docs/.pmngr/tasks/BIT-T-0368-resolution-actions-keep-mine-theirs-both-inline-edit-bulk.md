---
id: BIT-T-0368
type: task
title: "Resolution actions: keep mine/theirs/both, inline edit, bulk per page, shortcuts"
status: done
priority: high
parent: BIT-US-0054
milestone: BIT-M-0004
author: mcp
labels: [bitacora-app, merge, ui]
estimate: 3
created: 2026-10-06T14:34:50Z
updated: 2026-10-06T20:37:17Z
closed: 2026-10-06T20:37:17Z
---

## Description
`crates/bitacora-app/src/views/conflicts/actions.rs`: buttons + keymap (e.g. `1` mine, `2` theirs, `3` both, `e` edit, `j/k` next/prev) calling `MergeState::resolve`; Keep both inserts theirs as next sibling, regenerating `id::` for the copy only if the original had one; Edit opens a BlockEditor seeded with ours content and shows theirs/base read-only above (no markers); "Resolve all on this page with mine/theirs"; editing a conflicted block in the normal page editor marks it resolved with `Edit`. After the last resolution trigger the resolve commit through the engine.

## Acceptance Criteria
- `#[gpui::test]` for each action's resulting `Resolution`, bulk resolve count, and editor-edit counting as resolution.

## Notes
Story BIT-US-0054. Implements BIT-SP-0006.R16, BIT-SP-0006.R15.
