---
id: BIT-SP-0007
type: spec
title: MCP server over Streamable HTTP
status: backlog
author: mcp
labels: [mcp, api]
created: 2026-10-06T14:21:35Z
updated: 2026-10-06T21:59:48Z
requirements:
  R1:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/server.rs
        - crates/bitacora-cli/src/main.rs
        - crates/bitacora-app/src/views/settings/agents.rs
        - crates/bitacora-app/src/app.rs
      tests:
        - crates/bitacora-mcp/tests/http.rs
        - crates/bitacora-app/src/session.rs#mcp_endpoint_starts_when_a_token_file_is_given
    verified: {rev: "sha256:dc80ed2bd67a10a2", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R2:
    status: backlog
    trace:
      code: [crates/bitacora-mcp/src/server.rs]
      tests: [crates/bitacora-mcp/tests/http.rs]
    verified: {rev: "sha256:4bed8046d1a7dcfa", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R3:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/guard.rs
        - crates/bitacora-mcp/src/tokens.rs
      tests:
        - crates/bitacora-mcp/tests/http.rs
        - crates/bitacora-mcp/src/tokens.rs
    verified: {rev: "sha256:167302a936a25e40", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R4:
    status: backlog
    trace:
      code: [crates/bitacora-mcp/src/guard.rs]
      tests:
        - crates/bitacora-mcp/tests/http.rs
        - crates/bitacora-mcp/src/guard.rs
    verified: {rev: "sha256:5f81203e6ddcd1e1", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R5:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/tokens.rs
        - crates/bitacora-runtime/src/live.rs
        - crates/bitacora-app/src/app.rs
      tests:
        - crates/bitacora-mcp/src/tokens.rs#keychain_holds_the_secrets_and_the_file_only_metadata
        - crates/bitacora-mcp/src/tokens.rs#inline_secrets_migrate_to_the_keychain
        - crates/bitacora-mcp/src/tokens.rs#unavailable_keychain_falls_back_to_the_file_and_never_loses_a_token
    verified: {rev: "sha256:509b00fe857bf2f3", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R6:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/tokens.rs
        - crates/bitacora-mcp/src/policy.rs
        - crates/bitacora-mcp/src/handler.rs#exec_write
        - crates/bitacora-app/src/views/settings/agents.rs
      tests:
        - crates/bitacora-mcp/src/tokens.rs
        - crates/bitacora-runtime/tests/mcp_write.rs#toggles_scopes_catalogue_rate_limit_and_protected_pages
        - crates/bitacora-app/src/views/settings/tests.rs#agents_tokens_are_created_shown_once_and_revoking_refuses_the_next_request
        - crates/bitacora-app/src/views/settings/tests.rs#write_toggles_apply_to_the_running_server_and_are_saved
    verified: {rev: "sha256:9419df24d87ddc2a", commit: 5580d94bce79b0a9fb12e95d667c3d435af37b17, at: 2026-10-06T19:56:30Z, by: claude}
  R7:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/bridge.rs
        - crates/bitacora-mcp/src/handler.rs#exec_write
        - crates/bitacora-runtime/src/live.rs
        - crates/bitacora-sync/src/engine.rs#note_agent_write
      tests:
        - crates/bitacora-runtime/tests/mcp_write.rs#update_block_reaches_disk_index_audit_and_undo_restores_exact_bytes
        - crates/bitacora-runtime/tests/mcp_write.rs#agent_write_is_committed_by_sync_as_kind_agent_with_the_token_name
        - crates/bitacora-sync/tests/sync_engine.rs#agent_writes_are_committed_as_kind_agent_with_the_client_trailer
    verified: {rev: "sha256:a533edfbc6042578", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R8:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/audit.rs
        - crates/bitacora-mcp/src/server.rs#undo_audit_entry
        - crates/bitacora-mcp/src/guard.rs
        - crates/bitacora-runtime/src/live.rs#agent_activity
        - crates/bitacora-app/src/views/agent_activity.rs
        - crates/bitacora-app/src/editing.rs#EditingGate
      tests:
        - crates/bitacora-mcp/src/audit.rs
        - crates/bitacora-runtime/tests/mcp_write.rs#audit_covers_reads_and_auth_failures_and_survives_restart
        - crates/bitacora-runtime/tests/mcp_write.rs#insert_move_properties_status_remove_and_group_undo
        - crates/bitacora-runtime/tests/mcp_write.rs#a_multi_block_insert_undoes_as_one_step
        - crates/bitacora-app/src/views/agent_activity.rs#tests
        - crates/bitacora-app/src/editor/tests.rs#the_edited_block_is_reported_busy_to_the_mcp_gate
    verified: {rev: "sha256:f0d581e75e42d718", commit: 5580d94bce79b0a9fb12e95d667c3d435af37b17, at: 2026-10-06T19:56:30Z, by: claude}
  R9:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/bridge.rs#check_version
        - crates/bitacora-mcp/src/bridge.rs#resolve
      tests:
        - crates/bitacora-runtime/tests/mcp_write.rs#optimistic_concurrency_and_validation_errors
    verified: {rev: "sha256:bfddc10d2d02fae7", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R10:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/bridge.rs#guard_page
        - crates/bitacora-mcp/src/policy.rs#WriteGate
      tests:
        - crates/bitacora-mcp/src/bridge.rs#busy_and_conflicted_blocks_refuse_writes_and_write_nothing
    verified: {rev: "sha256:b54d5f700e5b19ce", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R11:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/tools.rs
        - crates/bitacora-mcp/src/handler.rs
        - crates/bitacora-mcp/src/index_reader.rs
        - crates/bitacora-mcp/src/query.rs
        - crates/bitacora-mcp/src/reader.rs
      tests:
        - crates/bitacora-mcp/tests/read_tools.rs
        - crates/bitacora-mcp/tests/read_tools.rs#query_tool_routes_simple_and_advanced_queries
        - crates/bitacora-cli/src/cmd/serve_tests.rs
    verified: {rev: "sha256:29e275453b975084", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R12:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/handler.rs
        - crates/bitacora-mcp/src/write_tools.rs
        - crates/bitacora-mcp/src/bridge.rs
      tests:
        - crates/bitacora-runtime/tests/mcp_write.rs#insert_move_properties_status_remove_and_group_undo
        - crates/bitacora-runtime/tests/mcp_write.rs#create_page_append_today_rename_with_links_delete_and_undo
        - crates/bitacora-runtime/tests/mcp_write.rs#toggles_scopes_catalogue_rate_limit_and_protected_pages
    verified: {rev: "sha256:2f63eb7571812462", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R13:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/bridge.rs#validate_content
        - crates/bitacora-mcp/src/write_tools.rs#collect_blocks
      tests:
        - crates/bitacora-mcp/src/bridge.rs
        - crates/bitacora-mcp/src/write_tools.rs
        - crates/bitacora-runtime/tests/mcp_write.rs#optimistic_concurrency_and_validation_errors
    verified: {rev: "sha256:c098df5463e537f0", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R14:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/policy.rs
        - crates/bitacora-mcp/src/bridge.rs#guard_page
      tests:
        - crates/bitacora-mcp/src/policy.rs
        - crates/bitacora-runtime/tests/mcp_write.rs#toggles_scopes_catalogue_rate_limit_and_protected_pages
    verified: {rev: "sha256:f62456ef79b098d1", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R15:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/index_reader.rs#read_asset
        - crates/bitacora-mcp/src/resources.rs
        - crates/bitacora-mcp/src/write_tools.rs
      tests:
        - crates/bitacora-mcp/tests/read_tools.rs#resources_templates_read_and_confinement
        - crates/bitacora-runtime/tests/mcp_write.rs#catalogue_has_no_shell_or_raw_path_tools
    verified: {rev: "sha256:57c26e5f84902468", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R16:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/render.rs
        - crates/bitacora-mcp/src/handler.rs
      tests:
        - crates/bitacora-mcp/tests/read_tools.rs#tools_list_declares_schemas_and_read_only_hints
        - crates/bitacora-mcp/src/render.rs
    verified: {rev: "sha256:d0b276131cc45046", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R17:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/handler.rs
        - crates/bitacora-mcp/src/compat.rs
      tests:
        - crates/bitacora-mcp/tests/read_tools.rs#tools_list_declares_schemas_and_read_only_hints
        - crates/bitacora-runtime/tests/mcp_write.rs#compat_api_is_off_by_default_and_audited_when_on
    verified: {rev: "sha256:a4a1f173c3c618fb", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R18:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/resources.rs
        - crates/bitacora-mcp/src/handler.rs
      tests:
        - crates/bitacora-mcp/tests/read_tools.rs#resources_templates_read_and_confinement
        - crates/bitacora-mcp/tests/read_tools.rs#page_changes_notify_subscribed_resources_within_two_seconds
    verified: {rev: "sha256:2446d9ac50f7a314", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R19:
    status: backlog
    trace:
      code: [crates/bitacora-mcp/src/prompts.rs]
      tests: [crates/bitacora-mcp/tests/read_tools.rs#prompts_list_and_get]
    verified: {rev: "sha256:89ad5efb1f22edf8", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R20:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/handler.rs
        - crates/bitacora-mcp/src/server.rs
      tests: [crates/bitacora-mcp/tests/http.rs]
    verified: {rev: "sha256:9fbc02ac35cbf054", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
---

## Purpose
AI agents can search, read and (when allowed) edit the graph through an always-on, secure MCP endpoint over HTTP.

## Scope
Transport, lifecycle, authentication and Origin checks, scopes, tools, resources, prompts, audit and undo integration. Source: [[mcp-server]]. Implemented by BIT-EP-0010.

## Requirements

### BIT-SP-0007.R1 — Serve MCP over Streamable HTTP independent of any window

The system SHALL serve the Model Context Protocol over Streamable HTTP (not stdio) at `http://127.0.0.1:<mcp.port>/mcp` (default port `12316`) from the running Bitacora process (desktop app, including tray mode, or `bitacora-cli serve`), independently of whether any UI window is open. If the configured port is busy the server SHALL fail visibly (settings + status) and SHALL NOT silently pick another port. An unauthenticated `GET /health` SHALL return only `{"ok":true}`.

#### Scenario: Agent connects while the main window is closed
- GIVEN Bitacora runs in tray mode with no window open and `mcp.enabled = true`
- WHEN a client sends `POST /mcp` with an `initialize` JSON-RPC request and a valid bearer token
- THEN the response is HTTP 200 with a JSON-RPC result containing `serverInfo.name = "bitacora"` and capabilities `tools`, `resources`, `prompts`

#### Scenario: Port already in use
- GIVEN another process listens on 127.0.0.1:12316
- WHEN Bitacora starts
- THEN the MCP status in Settings > Agents shows "Port 12316 is in use" and no listener is opened on any other port

#### Scenario: Health probe
- WHEN a client sends `GET /health` without `Authorization`
- THEN the response is `200 {"ok":true}` and contains no graph or version data

### BIT-SP-0007.R2 — Bind to loopback interfaces only

The server SHALL bind only to `127.0.0.1` and `::1` by default. Binding to any other interface SHALL require an explicit advanced setting and is out of scope for v1 (no TLS).

#### Scenario: Default bind addresses
- GIVEN default settings
- WHEN the server starts
- THEN the open listening sockets are exactly `127.0.0.1:12316` and `[::1]:12316`

#### Scenario: LAN access is refused
- GIVEN the machine has LAN address 192.168.1.20
- WHEN another host connects to `192.168.1.20:12316`
- THEN the TCP connection is refused because nothing listens on that address

### BIT-SP-0007.R3 — Always require a bearer token

Every request to `/mcp` (and `/api` if enabled) SHALL carry `Authorization: Bearer <token>` matching a configured, non-revoked token, compared in constant time. If no token is configured the server SHALL refuse all requests; it SHALL never be "open when unset". Rejections SHALL use HTTP 401 with `WWW-Authenticate: Bearer` and no JSON-RPC processing.

#### Scenario: Missing token
- WHEN a client sends `POST /mcp` with body `{"jsonrpc":"2.0","id":1,"method":"tools/list"}` and no `Authorization` header
- THEN the response is `401 Unauthorized` with header `WWW-Authenticate: Bearer` and the handler is not invoked

#### Scenario: Wrong token
- GIVEN the configured token is `tkA…`
- WHEN a client sends `Authorization: Bearer wrong`
- THEN the response is `401` and an audit entry `auth_failed` is recorded without the presented token value

#### Scenario: Revoked token
- GIVEN token "Cursor" was revoked in settings
- WHEN a request uses that token
- THEN the response is `401`

#### Scenario: Token list empty
- GIVEN all tokens were deleted
- WHEN any request reaches `/mcp`
- THEN the response is `401` and the settings page shows "MCP disabled: no tokens"

### BIT-SP-0007.R4 — Validate Origin and Host headers; no permissive CORS

The server SHALL reject (HTTP 403) any request whose `Origin` header is present and not in `mcp.allowed_origins` (absent or `null` Origin is allowed for native clients), SHALL reject requests whose `Host` is not `127.0.0.1:<port>`, `[::1]:<port>` or `localhost:<port>` (DNS-rebinding protection), and SHALL NOT emit `Access-Control-Allow-Origin: *` or any CORS header for non-allowlisted origins.

#### Scenario: Browser page from a foreign origin
- WHEN a request arrives with `Origin: https://evil.example` and a valid token
- THEN the response is `403 Forbidden` and contains no `Access-Control-Allow-Origin` header

#### Scenario: DNS rebinding
- WHEN a request arrives with `Host: attacker.example:12316`
- THEN the response is `403`

#### Scenario: Native client without Origin
- WHEN Claude Code sends `POST /mcp` with `Host: 127.0.0.1:12316`, no `Origin` and a valid token
- THEN the request is processed normally

#### Scenario: Preflight from unknown origin
- WHEN an `OPTIONS /mcp` preflight arrives with `Origin: http://localhost:3000` not in the allowlist
- THEN the response is `403` without CORS allow headers

### BIT-SP-0007.R5 — Store tokens outside the git-synced graph

Tokens SHALL be 256-bit random values generated on first run and stored in the OS keychain (`keyring` crate) with a fallback file in the app config directory with permissions `0600`. Tokens SHALL never be written inside the graph folder (which is git-synced) nor logged in clear text.

#### Scenario: First run
- GIVEN no token exists
- WHEN the app starts for the first time
- THEN a default token named "default" with 43+ base64url characters is created in the keychain entry `bitacora/mcp/default`

#### Scenario: Keychain unavailable
- GIVEN a Linux session without Secret Service
- WHEN a token is created
- THEN it is stored in `<config_dir>/bitacora/mcp-tokens.json` with mode `0600` and a warning is shown

#### Scenario: Graph never contains tokens
- GIVEN tokens exist
- WHEN `git grep` searches the graph repo for any token value
- THEN no match is found

### BIT-SP-0007.R6 — Named per-client tokens with scopes, read-only by default

The system SHALL support multiple named tokens, each with a scope set from `read`, `write`, `delete`, revocable from settings. Global settings `mcp.allow_writes` and `mcp.allow_deletes` SHALL default to `false`, so the server is read-only until the user enables writes. A write tool called without the needed scope or with writes disabled SHALL return a tool error (`isError: true`) with code `READ_ONLY` or `FORBIDDEN_SCOPE` and SHALL NOT change any file.

#### Scenario: Writes disabled by default
- GIVEN a fresh install with token "default" (scopes read, write)
- WHEN the agent calls `append_block {"page":"today","content":"hello"}`
- THEN the result is `isError: true` with `code: "READ_ONLY"` and today's journal is unchanged

#### Scenario: Missing delete scope
- GIVEN `mcp.allow_writes = true`, `mcp.allow_deletes = true` and token "Cursor" with scopes `[read, write]`
- WHEN it calls `remove_block {"uuid":"6650e1f2-…"}`
- THEN the result is `isError: true` with `code: "FORBIDDEN_SCOPE"`

#### Scenario: Tool list reflects scope
- GIVEN a token with scope `[read]`
- WHEN it calls `tools/list`
- THEN write and delete tools are either omitted or annotated as unavailable, and read tools carry `readOnlyHint: true`

### BIT-SP-0007.R7 — Route agent writes through the core op pipeline with undo and agent commits

Every MCP write SHALL be submitted to the `bitacora-core` command queue as `Op` transaction(s) with `origin = Agent{token_name}`; it SHALL NOT write files directly. Each tool call SHALL form exactly one undo transaction labelled with the agent, SHALL update the SQLite index, notify open editors, and mark the graph dirty so the sync engine commits it with `Bitacora-Kind: agent` and a `Bitacora-Agent: <client name>` trailer.

#### Scenario: Undo an agent edit from the UI
- GIVEN the agent called `update_block {"uuid":"u1","content":"New text"}` on page "Project X"
- WHEN the user presses Undo in the app
- THEN block u1 has its previous content and the undo label read "Undo agent edit (Claude Desktop): updated 1 block on Project X"

#### Scenario: Agent commit trailer
- GIVEN git sync is enabled
- WHEN the idle debounce commits the agent's change
- THEN the commit body contains `Bitacora-Kind: agent` and `Bitacora-Agent: Claude Desktop`

#### Scenario: User typing transaction is not merged
- GIVEN the user is typing in block u2 (open transaction)
- WHEN the agent appends a block to the same page
- THEN the agent op is a separate undo entry and the user's typing transaction is unaffected

### BIT-SP-0007.R8 — Audit every tool call with per-entry undo

The system SHALL append one JSONL record per tool call (and per auth failure) to an append-only audit log in the app data dir with: timestamp, token name, `initialize` clientInfo, tool name, args hash and short summary, affected block uuids, result (ok / error code). The UI SHALL list these entries ("Agent activity") and offer one-click undo for write entries whose blocks have not been changed since.

#### Scenario: Write call audited
- WHEN token "Claude Desktop" calls `insert_block {"target_uuid":"u1","content":"x","position":"after"}` successfully
- THEN the audit log gains a line like `{"ts":"2026-10-06T10:00:00Z","token":"Claude Desktop","client":{"name":"claude-ai","version":"…"},"tool":"insert_block","args_sha256":"…","summary":"insert 1 block after u1","blocks":["u9"],"result":"ok"}`

#### Scenario: Undo from activity list
- GIVEN the entry above is the latest change to u9
- WHEN the user clicks Undo on that entry
- THEN block u9 is removed through a normal undoable op and the entry is marked "undone"

#### Scenario: Read calls audited without content
- WHEN the agent calls `search {"query":"secret"}`
- THEN the audit entry stores the args hash and summary "search (1 arg)" but not result content

### BIT-SP-0007.R9 — Optimistic concurrency with expected_version

Every block and page returned by a tool SHALL carry a `version` (monotonic per block) and a page `etag`. Write tools SHALL accept an optional `expected_version`; on mismatch they SHALL return `isError: true` with `code: "CONFLICT"` and the current block content and version, and SHALL NOT apply the change.

#### Scenario: Stale version
- GIVEN block u1 has `version: 7`
- WHEN the agent calls `update_block {"uuid":"u1","content":"B","expected_version":6}`
- THEN the result is `{"isError":true,"code":"CONFLICT","current":{"uuid":"u1","content":"A","version":7}}` and the file is unchanged

#### Scenario: Matching version
- WHEN the agent calls `update_block {"uuid":"u1","content":"B","expected_version":7}`
- THEN the block is updated and the result carries `version: 8`

### BIT-SP-0007.R10 — Refuse writes to blocks being edited or in sync conflict

A write targeting a block that the user is currently editing SHALL return `code: "BLOCK_BUSY"` with a `retry_after_ms` hint; a write targeting a block with an unresolved sync conflict SHALL return `code: "BLOCK_IN_CONFLICT"`. Writes to other blocks on the same page SHALL proceed.

#### Scenario: User is editing the block
- GIVEN the editor cursor is inside block u1
- WHEN the agent calls `update_block {"uuid":"u1","content":"x"}`
- THEN the result is `isError: true`, `code: "BLOCK_BUSY"`, `retry_after_ms: 2000`

#### Scenario: Sibling block on the same page
- GIVEN the user edits u1 on page P
- WHEN the agent calls `update_block` on sibling u2 of page P
- THEN the update succeeds and the editor keeps u1's unsaved text

#### Scenario: Block in sync conflict
- GIVEN the sync engine is `Conflicted` with a content conflict on block u3
- WHEN the agent calls `set_block_property {"uuid":"u3","key":"status","value":"done"}`
- THEN the result is `code: "BLOCK_IN_CONFLICT"`

### BIT-SP-0007.R11 — Expose the minimum read tool set

The server SHALL expose the read tools `search`, `get_page`, `get_page_blocks_tree` (alias `get_block_tree` with a uuid), `get_block`, `list_pages`, `list_journals`, `get_today_journal`, `backlinks`, `tasks`, `query` (Logseq simple-query DSL and the supported advanced Datalog subset), `get_graph_info`, `list_graphs` and `git_sync_status`, each accepting an optional `graph` argument (default: active graph), with page names resolved case-insensitively through aliases.

#### Scenario: Get page via alias
- GIVEN page "Project X" has `alias:: PX`
- WHEN the agent calls `get_page {"name":"px"}`
- THEN the result has `name: "project x"`, `original_name: "Project X"`, `aliases: ["PX"]`, `block_count`, `etag`

#### Scenario: Simple query
- GIVEN three `TODO` blocks referencing `[[Project X]]` in the last 7 days
- WHEN the agent calls `query {"dsl":"(and (task TODO) [[Project X]] (between -7d today))"}`
- THEN the result lists those three blocks with uuid, page and breadcrumb

#### Scenario: Unknown page
- WHEN the agent calls `get_page {"name":"Nope"}`
- THEN the result is `isError: true` with `code: "NOT_FOUND"`

#### Scenario: Advanced Datalog query
- WHEN the agent calls `query {"dsl":"[:find (pull ?b [*]) :where [?b :block/marker \"TODO\"]]"}` using constructs in the supported subset
- THEN the query executes and the result lists the matching blocks

#### Scenario: Unsupported Datalog construct
- WHEN the agent calls `query` with an advanced query that uses a construct outside the supported subset
- THEN the result is `isError: true` with `code: "NOT_SUPPORTED"` and `unsupported: <construct>` naming it

#### Scenario: Malformed query
- WHEN the agent calls `query {"dsl":"[:find ?b :where"}`
- THEN the result is `isError: true` with `code: "INVALID_QUERY"`

### BIT-SP-0007.R12 — Expose the minimum write and delete tool set

The server SHALL expose write tools `create_page`, `append_block`, `prepend_block`, `insert_block` (positions `after|before|first_child|last_child`, single `content` or `blocks[]` tree), `update_block`, `set_block_property`, `remove_block_property`, `move_block`, `set_task_status`, `git_sync_now` (scope `write`) and delete tools `remove_block`, `rename_page`, `delete_page` (scope `delete`). Every block used as a write target SHALL get a persistent `id::` lazily, as Logseq does for referenced blocks.

#### Scenario: Append to today's journal
- GIVEN writes are enabled and today is 2026-10-06 with `:journal/file-name-format "yyyy_MM_dd"`
- WHEN the agent calls `append_block {"page":"today","content":"Call Ana #followup"}`
- THEN `journals/2026_10_06.md` ends with `- Call Ana #followup` and the result returns the new block uuid and version

#### Scenario: Insert a block tree as last child
- WHEN the agent calls `insert_block {"target_uuid":"u1","position":"last_child","blocks":[{"content":"A","children":[{"content":"A.1"}]}]}`
- THEN u1 gains child "A" with grandchild "A.1", serialized with the file's indentation style

#### Scenario: Rename page updates links
- GIVEN scope `delete` and deletes enabled
- WHEN the agent calls `rename_page {"name":"Old","new_name":"New"}`
- THEN the page file is renamed per the graph's file-name format and every `[[Old]]` becomes `[[New]]` in one undo transaction

### BIT-SP-0007.R13 — Validate agent-supplied block content

`content` SHALL be Logseq Markdown for exactly one block (first line plus continuation lines, no leading `- `). The server SHALL parse it with `bitacora-markdown` and return `code: "INVALID_CONTENT"` when it would split into multiple blocks, or when it contains reserved property lines (`id::`, `collapsed::`) not explicitly allowed.

#### Scenario: Content that would create two blocks
- WHEN the agent calls `update_block {"uuid":"u1","content":"first\n- second"}`
- THEN the result is `isError: true`, `code: "INVALID_CONTENT"`, message "content contains 2 blocks; use blocks[]"

#### Scenario: Reserved property
- WHEN the agent calls `append_block {"page":"P","content":"x\nid:: 1234"}`
- THEN the result is `code: "INVALID_CONTENT"`

#### Scenario: Multi-line block accepted
- WHEN the agent sends `content: "Title\nsecond line\nstatus:: open"`
- THEN one block is created with continuation line "second line" and property `status:: open`

### BIT-SP-0007.R14 — Rate-limit agent writes and protect configured pages

The server SHALL limit writes per token (default 60 write ops/min, at most 200 blocks per batch call) returning `code: "RATE_LIMITED"` with `retry_after_ms`, and SHALL refuse writes (`code: "FORBIDDEN_SCOPE"`) to pages with `bitacora-agent-readonly:: true` or under any namespace in `mcp.protected_namespaces`.

#### Scenario: Write burst
- GIVEN token "script" already performed 60 write ops in the last minute
- WHEN it calls `append_block` again
- THEN the result is `code: "RATE_LIMITED"` with `retry_after_ms > 0`

#### Scenario: Oversized batch
- WHEN the agent calls `create_page` with 250 blocks
- THEN the result is `code: "INVALID_CONTENT"` "max 200 blocks per call" and nothing is written

#### Scenario: Protected namespace
- GIVEN `mcp.protected_namespaces = ["private"]`
- WHEN the agent calls `append_block {"page":"private/diary","content":"x"}`
- THEN the result is `code: "FORBIDDEN_SCOPE"`

### BIT-SP-0007.R15 — No shell, git command or raw path execution for agents

The server SHALL NOT expose any tool that executes shell or arbitrary git commands (unlike Logseq's `exec_git_command`). No tool SHALL accept raw file paths for writing; page names map to files only through the core naming service, and `bitacora://asset/<path>` reads SHALL be restricted to the graph's `assets/` folder after path normalization.

#### Scenario: Tool catalogue
- WHEN the agent calls `tools/list`
- THEN no tool name or schema contains a free-form command or file-path write argument

#### Scenario: Asset path traversal
- WHEN the agent reads resource `bitacora://asset/../logseq/config.edn`
- THEN the result is an error `NOT_FOUND` and no file outside `assets/` is read

#### Scenario: Page name with traversal characters
- WHEN the agent calls `create_page {"name":"../../etc/passwd"}`
- THEN the page file is created inside `pages/` with an encoded file name per the graph's `:file/name-format`

### BIT-SP-0007.R16 — Return structured JSON plus Markdown text with annotations and paging

Tool results SHOULD include `structuredContent` validated by a published `outputSchema` and a compact Markdown text rendering. Read tools SHALL carry `readOnlyHint`, delete tools `destructiveHint`, `set_block_property` `idempotentHint`. List tools SHALL page with `cursor`, and text output SHALL be capped (~20k chars) with `truncated: true`.

#### Scenario: Blocks tree result
- WHEN the agent calls `get_page_blocks_tree {"name":"Project X"}`
- THEN the result contains `structuredContent.blocks[]` with `{uuid, content, properties, marker?, version, children[]}` and a `text` item with the page as Markdown

#### Scenario: Pagination
- GIVEN 1,200 pages
- WHEN the agent calls `list_pages {"limit":100}`
- THEN 100 summaries and a `next_cursor` are returned; passing it returns the next 100

#### Scenario: Truncation
- GIVEN a page whose Markdown is 80k chars
- WHEN `get_page_blocks_tree` is called
- THEN the text item is at most ~20k chars and `truncated: true`

### BIT-SP-0007.R17 — Mirror Logseq plugin API names; optional POST /api compatibility

Tool descriptions SHOULD name the Logseq plugin API they mirror (e.g. `Editor.getPageBlocksTree`, `Editor.insertBlock`, `DB.q`). The server SHOULD offer an optional `POST /api` endpoint (off by default, same auth and Origin rules) that accepts Logseq's `{"method":"logseq.Editor.getBlock","args":[…]}` and dispatches to the same handlers; UI-only and git/shell methods SHALL be refused.

#### Scenario: Compatibility call
- GIVEN `mcp.compat_api = true`
- WHEN a script sends `POST /api` with `Authorization: Bearer <token>` and `{"method":"logseq.Editor.getPage","args":["Project X"]}`
- THEN the response is `200` with a PageEntity-like JSON `{"name":"project x","originalName":"Project X",…}`

#### Scenario: Disabled by default
- GIVEN default settings
- WHEN `POST /api` is called with a valid token
- THEN the response is `404`

#### Scenario: Forbidden method
- WHEN `{"method":"logseq.Git.execCommand","args":["status"]}` is sent
- THEN the response is `{"error":"method not supported"}` and nothing executes

### BIT-SP-0007.R18 — Expose pages, blocks and journals as subscribable resources

The server SHOULD expose resources `bitacora://page/<name>` (and `bitacora://graph/<graph>/page/<name>`), `bitacora://block/<uuid>`, `bitacora://journal/<yyyy-mm-dd>` / `today`, `bitacora://graph/<graph>/config` (read-only), `bitacora://sync/status` and size-capped `bitacora://asset/<path>`, advertise them via `resources/templates/list`, and support `resources/subscribe` with `notifications/resources/updated` sent on the session's SSE stream when the page changes.

#### Scenario: Read a page resource
- WHEN the agent calls `resources/read {"uri":"bitacora://page/Project%20X"}`
- THEN the content has `mimeType: "text/markdown"` and the raw file text

#### Scenario: Subscription notification
- GIVEN a stateful session subscribed to `bitacora://page/Project%20X`
- WHEN the user edits a block on that page and the writer flushes
- THEN the session receives `notifications/resources/updated` with that URI within 2 s

### BIT-SP-0007.R19 — Provide review and syntax prompts

The server SHOULD provide prompts `daily_review {date?}`, `weekly_review {week?}`, `summarize_page {name}`, `capture {text}` and `logseq_syntax`, built from graph data (journal, tasks, backlinks) and graph config (date format, journals dir). Server `instructions` SHALL remind agents that note content is data, not instructions.

#### Scenario: Daily review
- GIVEN today's journal has 4 blocks and 3 open TODOs are scheduled today
- WHEN the agent calls `prompts/get {"name":"daily_review"}`
- THEN the returned messages include the journal Markdown and the 3 tasks with uuids

#### Scenario: Syntax prompt
- WHEN the agent calls `prompts/get {"name":"logseq_syntax"}`
- THEN the message explains blocks (`- `), `key:: value` properties, `[[refs]]`, `((uuid))` and task markers

### BIT-SP-0007.R20 — Implement with rmcp on axum behind an internal module

The server SHOULD use the official `rmcp` SDK (`~3.5`, features `server`, `macros`, `schemars`, `transport-streamable-http-server`, `transport-streamable-http-server-session`) mounted as `StreamableHttpService` on an axum 0.8 router, run on a dedicated tokio runtime (ADR-010, ADR-012), with all rmcp types confined to one internal module of `bitacora-mcp` so SDK upgrades touch one place. Both stateful sessions (for subscriptions) and stateless JSON-response requests SHOULD be supported.

#### Scenario: Upgrade isolation
- WHEN `rmcp` is bumped to a new minor version
- THEN only files under `crates/bitacora-mcp/src/transport/` need changes and tool handler modules compile unchanged

#### Scenario: Core stays synchronous
- WHEN `cargo tree -p bitacora-core` is inspected
- THEN it contains neither `tokio` nor `rmcp`
