---
id: BIT-T-0133
type: task
title: Implement block-level Op variants with apply and inverse
status: in_progress
priority: critical
parent: BIT-US-0029
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, core]
estimate: 5
created: 2026-10-06T14:29:59Z
updated: 2026-10-06T18:28:39Z
started: 2026-10-06T18:28:39Z
---

## Description
In `crates/bitacora-core/src/ops.rs` implement `Op::{InsertSubtree, RemoveSubtree, Move, SetText, EditText, AdoptChildren}` with `apply(&mut self, &mut Graph) -> Result<(), OpError>` (filling `captured`/`removed`) and `inverse(&self) -> Op`. `Subtree` is a detached owned tree (blocks + origins) so a removed clean subtree re-inserted by undo stays clean. `Move` supports cross-page positions and rejects moving into own descendant. `EditText` validates the range is on char boundaries and `removed` matches.

## Acceptance Criteria
- Unit test per variant: apply then inverse restores the tree and texts; double inverse equals original op.
- `RemoveSubtree` + inverse keeps `Origin`s so the serialized bytes are identical.
- `OpError` variants: `UnknownBlock`, `InvalidPosition`, `Cycle`, `TextMismatch`.

## Notes
Story BIT-US-0029. Implements BIT-SP-0004.R5. See [[block-editor]] §3.1.
