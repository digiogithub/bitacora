---
id: BIT-US-0029
type: story
title: Core document model, Op set and transactions
status: backlog
priority: critical
parent: BIT-EP-0007
milestone: BIT-M-0003
author: mcp
labels: [editor, core, bitacora-core]
estimate: 8
created: 2026-10-06T14:28:07Z
updated: 2026-10-06T14:28:07Z
---

## Description
As a developer of Bitacora, I want an in-memory page model with session-stable block ids and a small set of invertible primitive ops grouped into atomic transactions, so that every editor command, MCP write and external change can be applied, undone and written through one well-tested path.

This is the foundation for every other story in the epic: `Graph`, `Page`, `Block`, `Origin`, `DiskSnapshot`, `FileStyle` (see [[block-editor]] §2) and the `Op` enum / `Transaction` / `Graph::commit` (§3).

## Acceptance Criteria
- `Page` loaded from bytes builds a block tree with `BlockId`s and `Origin { span, depth, text_hash, layout }` for every block.
- `Block::is_clean()` holds for untouched blocks, including after same-depth moves.
- All primitive ops (`InsertSubtree`, `RemoveSubtree`, `Move`, `SetText`, `EditText`, `AdoptChildren`, `SetPreamble`, `CreatePage`, `DeletePage`, `RenameFile`) implement `apply` and `inverse`.
- `Graph::commit` applies a transaction atomically (rollback on failure) and checks tree invariants (acyclic, single parent slot, unique uuids, no move into own descendant).
- Commands are pure `plan(&Graph, Cmd) -> Result<Vec<Op>, Refusal>` functions.
- Property test: for random op sequences, applying inverses in reverse restores the original serialized bytes.

## Notes
Implements: BIT-SP-0004.R4, BIT-SP-0004.R5.
See [[block-editor]] §2–3, [[04-editor-outliner-operations]] §1, §3.1. ADR-002, ADR-006, ADR-012 (core is synchronous). Depends on the span-preserving serializer from BIT-EP-0003.
