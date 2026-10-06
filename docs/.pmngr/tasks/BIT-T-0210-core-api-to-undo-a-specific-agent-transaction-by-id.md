---
id: BIT-T-0210
type: task
title: Core API to undo a specific agent transaction by id
status: backlog
priority: high
parent: BIT-US-0022
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, audit, undo]
estimate: 2
created: 2026-10-06T14:31:00Z
updated: 2026-10-06T14:31:00Z
---

## Description
In `bitacora-core` undo module: `UndoLog::revert_transaction(tx_id) -> Result<TxResult, RevertError>` that builds and submits the inverse ops of a past transaction as a new undoable transaction, refusing with `RevertError::ChangedSince{blocks}` if any affected block's version moved after `tx_id`. Expose `can_revert(tx_id)` for the UI.

## Acceptance Criteria
- Unit tests: revert insert/update/move/delete; refusal when block changed later; revert itself is undoable.

## Notes
Story BIT-US-0022. Implements BIT-SP-0007.R8. See [[block-editor]] (transactions/undo).
