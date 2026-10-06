---
id: BIT-T-0200
type: task
title: "Agent write bridge: submit Op transactions to the core command queue"
status: done
priority: high
parent: BIT-US-0020
milestone: BIT-M-0003
author: mcp
labels: [bitacora-mcp, bitacora-core, write]
estimate: 3
created: 2026-10-06T14:31:00Z
updated: 2026-10-06T19:40:38Z
closed: 2026-10-06T19:40:38Z
---

## Description
`crates/bitacora-mcp/src/write/bridge.rs`: `AgentWriter::submit(ctx: &AuthContext, ops: Vec<Op>, expected: Option<Version>) -> Result<TxResult, ToolError>` that sends a `Transaction { origin: Origin::Agent { token_name, client }, ops, label }` to the core command queue (sync API) via `tokio::task::spawn_blocking` + oneshot reply. Core side (`bitacora-core`): ensure `Origin::Agent` is carried into undo entries ("Undo agent edit (<client>): …"), dirty marker for sync (`CommitKind::Agent{client}`), and editor change events with "edited by agent" flag. Map core errors: `VersionMismatch` → `CONFLICT` (with current block), `BlockBeingEdited` → `BLOCK_BUSY{retry_after_ms: 2000}`, `BlockInConflict` → `BLOCK_IN_CONFLICT`.

## Acceptance Criteria
- Unit tests with an in-memory core: one undo entry per call; agent ops don't merge with an open user typing transaction.
- `bitacora-core` gains no tokio dependency.

## Notes
Story BIT-US-0020. Implements BIT-SP-0007.R7, BIT-SP-0007.R9, BIT-SP-0007.R10. ADR-012. See [[04-editor-outliner-operations]].
