---
created_at: 2026-10-06T19:41:38.987403731Z
updated_at: 2026-10-06T19:41:38.987403731Z
tags:
    - change
    - mcp
    - write
    - audit
---
# MCP write/delete tools, policy, audit with undo, /api compat (BIT-US-0020..0023)

Tags: change, mcp, write, audit, security. Continues [[changes/bit-us-0017-0019-mcp-read-tools-resources-prompts.md]], [[changes/bit-us-0015-0016-mcp-server-skeleton-auth.md]], [[changes/bit-us-0029-0062-core-editor-model-ops-and-command-queue.md]], [[changes/bit-us-0067-runtime-crate-wiring.md]]. Design: [[mcp-server]] (implementation notes for the write side and the /api method table were added), plan [[bitacora-full-development-plan]].

## What changed
- `crates/bitacora-mcp/src/bridge.rs` (`QueueBridge`): write tools become **groups of core transactions** on the single-writer `CommandQueue` (`Source::Mcp`; `Cmd` planners + `Request::Commit`). All-or-nothing groups (rollback with inverse ops), page loading from disk, uuid -> block lookup (loaded pages, then index uuid + document position verified against the text), `expected_version` -> `CONFLICT {current}`, `BLOCK_BUSY` (`WriteGate`), `BLOCK_IN_CONFLICT` (`SyncStatus.conflict_pages`), content validation (`INVALID_CONTENT`), lazy persistent `id::`, `rename_page` (delete + create + link rewrite in one group; old file recycled), `delete_page` (core recycle), undo with a page-content fingerprint (refused with `Changed` after later edits).
- `write_tools.rs`: argument types and tool bodies. `handler.rs`: 13 new tools (`create_page`, `append_block`, `prepend_block`, `insert_block`, `update_block`, `set_block_property`, `remove_block_property`, `move_block`, `set_task_status`, `git_sync_now`, `remove_block`, `rename_page`, `delete_page`), `exec_write` policy chain (toggle -> scope -> rate limit), custom `call_tool` (audit of every call, `audit_id` in write results) and `list_tools` (catalogue filtered by toggles + scope).
- `policy.rs` (`WritePolicy`, `WriteGate`): `allow_writes`/`allow_deletes` default off (live-adjustable), 60 calls/min/token sliding window, 200 blocks/call, protected namespaces + `bitacora-agent-readonly:: true` (`PROTECTED_PAGE`).
- `audit.rs` (`AuditLog`): JSONL `audit.jsonl` (10 MiB x 5, `undone` events, no content/token values), per tool call, per `/api` call, per auth failure (`guard.rs`). In-memory undo data (500 calls).
- `compat.rs`: optional `POST /api` (`McpConfig::api_enabled`, 404 when off), same guard/scopes/audit; unsupported methods -> `{"error":"method not supported"}`.
- `server.rs`: `McpConfig` gains `allow_writes`, `allow_deletes`, `protected_namespaces`, `writes_per_minute`, `api_enabled`, `audit_dir`, `gate`; `ServerParts` + `McpServer::start_with`; `policy()`, `audit()`, `undo_audit_entry(id)`. `status.rs`: `SyncStatusProvider::{note_agent_write, sync_now}`. `reader.rs`/`index_reader.rs`: `GraphReader::block_position`. `render.rs`: new error codes and `extra` fields.
- `crates/bitacora-runtime`: wires `QueueBridge` + audit dir (`<data>/mcp-audit`), `Session::{mcp_policy, agent_activity, undo_agent_entry}`; `SlotStatus` forwards agent writes and sync-now to the engine.
- `crates/bitacora-sync/src/engine.rs`: `Command::AgentWrite`, `SyncEngine::note_agent_write` -> next auto commit is `Bitacora-Kind: agent` + `Bitacora-Agent` trailer (not squashed).
- `xtask/src/deps.rs`: allowed edges `bitacora-mcp -> bitacora-config, bitacora-markdown`.

## Why
Spec BIT-SP-0007 R6-R10, R12-R15, R17; AGENTS.md rule 7 (writes off by default, audited, undoable) and rule 3 (single writer).

## Verification
`cargo fmt --all`, `cargo clippy --workspace --all-targets --locked -- -D warnings` clean; `cargo test -p bitacora-mcp -p bitacora-runtime -p bitacora-sync --locked` all pass (mcp 33 unit + 10 http + 10 read_tools, runtime `mcp_write` 10 e2e tests, sync 75 unit + integration incl. the new agent-commit test); `cargo xtask check-deps` OK; `typos` clean. E2E (`crates/bitacora-runtime/tests/mcp_write.rs`): real server over a temp graph through the runtime: write -> atomic file -> index -> audit -> undo restores exact bytes; CONFLICT; scopes/toggles/catalogue/rate limit/protected pages; trees, move, properties, remove; create_page, today journal, rename with links, delete; /api; audit auth failures and no-content/no-token logs; agent commit trailer via a live sync engine.

## Limits / follow-ups
- "Agent activity" view (BIT-T-0211) is app UI, left open. The CLI `serve` has no flags for `allow_writes`/`allow_deletes`/`api_enabled` yet (McpConfig fields exist).
- `BLOCK_BUSY` needs the app to implement `WriteGate`; `BLOCK_IN_CONFLICT` is page-granular and the runtime's `conflict_pages` is empty today.
- Undo data is in memory only (entries survive restarts in the log, but are not undoable). `clientInfo` is only retained in stateful sessions.
- `get_today_journal create_if_missing` still answers `READ_ONLY` (read tool); journals are created via `append_block page=today` without template expansion.
