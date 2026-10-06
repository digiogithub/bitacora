# MCP server

## Summary

Bitacora runs an always-on **Model Context Protocol server over Streamable HTTP** inside the desktop process, bound to `127.0.0.1`, protected by a bearer token and Origin checks. Agents (Claude Desktop/Code, IDE agents, scripts) get tools to search, read and edit the graph using the **same operation pipeline as the UI** — every write is an outliner operation ([[04-editor-outliner-operations]]) that goes through validation, the file writer, the index ([[sqlite-index-schema]]), undo history and git auto-commit ([[git-sync-merge]]). Tool names and argument shapes deliberately mirror Logseq's plugin API (`getPage`, `getPageBlocksTree`, `insertBlock`, `updateBlock`, `moveBlock`, `q`, `search`, `getPageLinkedReferences`…, see [[05-git-and-apis]]) so existing Logseq users and agent prompts transfer. Implementation: the official Rust SDK **`rmcp` (3.5.x)** with `transport-streamable-http-server` mounted on an **axum** router.

---

## 1. SDK choice

| Option | Notes |
|---|---|
| **`rmcp`** (official, `modelcontextprotocol/rust-sdk`, v3.5.1 released 2026-10-05) | Default features `server`, `macros`, `schemars`, `base64`, `uuid`, `transport-async-rw`. Streamable HTTP server via feature **`transport-streamable-http-server`** (+ `transport-streamable-http-server-session` for stateful sessions). Exposes `StreamableHttpService` (a tower service: `axum::Router::new().nest_service("/mcp", service)`), `LocalSessionManager`, `StreamableHttpServerConfig` (stateless / JSON-response modes). Tools via `#[tool_router]`, `#[tool]`, `#[tool_handler]`; JSON Schemas from `schemars`. README states support for protocol `2026-07-28` with backward compatibility to `2025-11-25` and older. Has `auth` features (OAuth) for later. |
| `rust-mcp-sdk` (community) | Viable, includes an HTTP server (hyper), but non-official and tracks spec more slowly. |
| Hand-rolled JSON-RPC over axum | Full control, but we would re-implement session handling, SSE resumption, protocol negotiation, schema output. Not worth it. |

**Decision**: `rmcp` with features `["server", "macros", "schemars", "transport-streamable-http-server", "transport-streamable-http-server-session"]`, axum 0.8 router, tokio. Pin the minor version (`rmcp = "~3.5"`) — the crate has had breaking majors; wrap it behind our own `mcp` module so upgrades touch one place. Verify the exact type names against docs.rs when implementing.

Sketch:

```rust
#[derive(Clone)]
struct BitacoraMcp { app: AppHandle, client: ClientInfo, tool_router: ToolRouter<Self> }

#[tool_router]
impl BitacoraMcp {
    #[tool(description = "Full-text search over pages and blocks")]
    async fn search(&self, Parameters(p): Parameters<SearchArgs>) -> Result<CallToolResult, McpError> { ... }
    #[tool(description = "Insert a block relative to another block (Logseq: Editor.insertBlock)")]
    async fn insert_block(&self, Parameters(p): Parameters<InsertBlockArgs>) -> Result<CallToolResult, McpError> { ... }
}

#[tool_handler]
impl ServerHandler for BitacoraMcp { fn get_info(&self) -> ServerInfo { /* name, version, instructions, capabilities: tools+resources+prompts */ } }

let svc = StreamableHttpService::new(move || Ok(BitacoraMcp::new(app.clone())),
                                     LocalSessionManager::default().into(),
                                     StreamableHttpServerConfig::default());
let router = axum::Router::new()
    .nest_service("/mcp", svc)
    .layer(from_fn_with_state(auth_state, auth_and_origin_guard));
axum::serve(TcpListener::bind(("127.0.0.1", port)).await?, router).await?;
```

---

## 2. Process model and lifecycle

- The server lives in the Bitacora main process (tokio runtime), started at app launch and kept running while the app runs (including when the window is closed to tray — "always running"). Optional "start at login" puts the app in tray mode.
- It does **not** depend on a UI window (contrast with Logseq, whose HTTP API proxies into the renderer and dies with it — [[05-git-and-apis]] §2.2). Tools call the core services directly.
- Multiple graphs: every tool takes an optional `graph` argument (name or path); default = the active graph. `list_graphs` tool enumerates.
- Port: default **`12316`** (Logseq API uses 12315; avoid clashing if both run), configurable; if busy, fail visibly in settings (don't silently pick another — clients have it configured). Endpoint `http://127.0.0.1:12316/mcp`.
- Health: `GET /health` (no auth, returns only `{"ok":true}`) for tooling.
- Optional compatibility endpoint `POST /api` accepting Logseq's `{method:"logseq.Editor.getBlock", args:[...]}` mapped onto the same handlers (off by default).

---

## 3. Security

| Threat | Control |
|---|---|
| Other machines | bind `127.0.0.1` and `::1` only; LAN binding requires an explicit "advanced" toggle + TLS is out of scope v1 |
| Browser pages / DNS rebinding (MCP spec requires Origin validation) | reject requests whose `Origin` header is present and not in the allowlist (`null`/absent allowed for native clients); validate `Host` is `127.0.0.1:<port>`/`localhost:<port>`; **no permissive CORS** (Logseq uses `origin: "*"`) |
| Unauthenticated local processes | **mandatory** `Authorization: Bearer <token>`; token = 256-bit random, generated on first run, stored in the OS keychain (`keyring` crate) with a fallback to the app config dir (0600), **never inside the graph** (the graph is git-synced). No token ⇒ server refuses all requests (never "open when unset", unlike Logseq) |
| Multiple clients | per-client tokens with names ("Claude Desktop", "Cursor") and scopes; revocable in settings; token name recorded in audit log and commit trailers |
| Destructive agent writes | permission levels per token: `read`, `write` (create/append/update/move), `delete` (remove block/page, rename page). Global "Agent writes" toggle (default: **read-only** until the user enables writes). Optional "confirm in UI" mode: writes queue a toast the user approves |
| Mass edits | rate limits (e.g. 60 write ops/min/token, max 200 blocks per batch call); pages with `bitacora-agent-readonly:: true` property or under configured namespaces are protected |
| Prompt injection via note content | tool results are data; server `instructions` remind the agent of that; no tool executes shell/git commands (Logseq exposes `exec_git_command` — Bitacora does not) |
| Path traversal | page names map to files only through the core naming service; no tool takes raw file paths for writing; `read_asset` restricted to `assets/` |
| Audit | append-only JSONL audit log in app data dir: timestamp, token name, client info (`initialize` clientInfo), tool, args hash + summary, affected block uuids, result; viewable in UI ("Agent activity") with one-click undo per entry |

Optional later: OAuth 2.1 per MCP auth spec via `rmcp` `auth` feature, for remote agents.

---

## 4. Write pipeline and concurrency with the UI

```mermaid
sequenceDiagram
    participant A as Agent (MCP client)
    participant M as MCP handler (rmcp)
    participant O as Op pipeline (outliner)
    participant W as File writer + index
    participant U as UI / editor
    participant G as Git sync engine
    A->>M: tools/call update_block {uuid, content, expected_version}
    M->>M: auth, scope, rate limit, validate args
    M->>O: Op::UpdateBlock (origin = Agent{token})
    O->>O: acquire graph write lock, check version (optimistic)
    O->>W: apply → serialize page → atomic write (tmp + rename)
    W->>W: reindex SQLite, bump block versions
    W-->>U: change event (re-render, keep cursor; flag "edited by agent")
    W-->>G: dirty → idle-debounced commit (Kind: agent)
    O-->>M: result {block, version}
    M-->>A: CallToolResult (structured + text)
```

- **Single op pipeline**: MCP writes are the same `Op` values the editor emits (`InsertBlocks`, `UpdateBlock`, `MoveBlocks`, `DeleteBlocks`, `SetProperty`, `CreatePage`…), so validation, normalization (`id::` assignment, property serialization), undo grouping and indexing are identical.
- **Undo**: each tool call is one undo transaction tagged `origin=agent:<token-name>`; the UI's undo stack includes it ("Undo agent edit: updated 1 block on Project X"). Agent writes do not collapse the user's own in-flight typing transaction.
- **Optimistic concurrency**: every block/page returned carries `version` (monotonic per block, from the index) and a page `etag`. Write tools accept optional `expected_version`; mismatch → error `CONFLICT` with the current content, so agents re-read instead of clobbering. If the user is **currently editing** the target block, the write is rejected with `BLOCK_BUSY` (retry-after hint) rather than racing the editor buffer; other blocks on the same page can be written (the editor merges external changes per block).
- **Sync conflicts**: while [[git-sync-merge]] is `Conflicted`, writes to blocks with pending conflicts return `BLOCK_IN_CONFLICT`; other writes proceed.
- **Ordering**: ops are serialized per graph through the write lock; reads use the SQLite index snapshot (WAL) and never block on writes.

---

## 5. Tools

Naming: snake_case MCP names; each lists the Logseq plugin API it mirrors. All take optional `graph`. Block identity is the `uuid` (Bitacora ensures every block returned to an agent has a persistent `id::` when it's used as a write target — written lazily, like Logseq does for referenced blocks). Outputs are structured JSON (`structuredContent` with an `outputSchema`) plus a compact Markdown text rendering for models that read text.

### 5.1 Read

| Tool | Args | Returns | Logseq equivalent |
|---|---|---|---|
| `search` | `query`, `limit=20`, `kind? (page\|block\|all)`, `page?` (restrict) | hits: `{type, uuid?, page, snippet, score, breadcrumb}` | `logseq.search`, `App.search` |
| `get_page` | `name` (or alias, case-insensitive) | `{name, original_name, uuid, properties, aliases, journal_day?, file, updated_at, block_count, etag}` | `Editor.getPage` |
| `get_page_blocks_tree` (alias `get_block_tree` when given a uuid) | `name` \| `uuid`, `max_depth?`, `include_properties=true`, `collapsed_children=true` | nested `{uuid, content, properties, marker?, children[]}`, plus `markdown` rendering | `Editor.getPageBlocksTree`, `getCurrentPageBlocksTree`, `getBlock({includeChildren:true})` |
| `get_block` | `uuid`, `include_children=false`, `include_parents=false` | block + breadcrumb | `Editor.getBlock` |
| `list_pages` | `namespace?`, `tag?`, `modified_since?`, `limit`, `cursor` | page summaries | `Editor.getAllPages`, `getPagesFromNamespace` |
| `list_journals` | `from?`, `to?`, `limit=7` | journal pages with day + block count | — |
| `get_today_journal` | `create_if_missing=false` | page tree for today (date format from config) | — (`App.onTodayJournalCreated`) |
| `backlinks` | `name` \| `uuid`, `include_unlinked=false` | grouped by page: referencing blocks with breadcrumb | `Editor.getPageLinkedReferences` |
| `tasks` | `status? [TODO,DOING,NOW,LATER,WAITING,DONE,CANCELED]`, `page?`, `scheduled_before?`, `deadline_before?`, `priority?`, `limit` | task blocks | — (common `q` use) |
| `query` | `dsl` (Logseq simple query, e.g. `(and (task TODO) [[Project X]] (between -7d today))`) | matching blocks/pages | `DB.q` |
| `get_graph_info` | — | name, path, page/block counts, config summary (date format, journals dir) | `App.getCurrentGraph`, `getCurrentGraphConfigs` |
| `list_graphs` | — | known graphs | — |
| `git_sync_status` | — | state, ahead/behind, last sync, conflicts (count + pages), last error | — |

`query` supports the Logseq simple-query DSL subset that maps to SQL over the index ([[sqlite-index-schema]]): `and/or/not`, `[[page]]`, `#tag`, `(task …)`, `(priority …)`, `(page-property …)`, `(property …)`, `(between …)`, `(page …)`, `(full-text-search …)` / `"text"`, `(sort-by …)`. Raw Datalog (`datascriptQuery`) is **not** supported (no Datascript); a read-only `sql_query` tool may be exposed behind an "advanced" scope against a documented view schema.

### 5.2 Write (scope `write`)

| Tool | Args | Logseq equivalent |
|---|---|---|
| `create_page` | `name`, `properties?`, `blocks?` (IBatchBlock-like `[{content, properties?, children?}]`), `journal?`, `if_exists = error\|return` | `Editor.createPage` |
| `append_block` | `page` (name, or `"today"`), `content`, `properties?`, `children?` | `Editor.appendBlockInPage` |
| `prepend_block` | same | `Editor.prependBlockInPage` |
| `insert_block` | `target_uuid`, `content` \| `blocks[]`, `position = after\|before\|first_child\|last_child`, `properties?` | `Editor.insertBlock` (`sibling`, `before`) / `insertBatchBlock` |
| `update_block` | `uuid`, `content`, `properties?` (merge), `expected_version?` | `Editor.updateBlock` |
| `set_block_property` / `remove_block_property` | `uuid`, `key`, `value?` | `Editor.upsertBlockProperty` / `removeBlockProperty` |
| `move_block` | `uuid`, `target_uuid`, `position` (same enum), `expected_version?` | `Editor.moveBlock` (`before`, `children`) |
| `set_task_status` | `uuid`, `status` | — (marker cycling) |
| `git_sync_now` | — (scope `write`) | — |

### 5.3 Delete / restructure (scope `delete`)

| Tool | Args | Logseq equivalent |
|---|---|---|
| `remove_block` | `uuid`, `expected_version?` (removes children) | `Editor.removeBlock` |
| `rename_page` | `name`, `new_name`, `update_links=true` | `Editor.renamePage` |
| `delete_page` | `name` (moved to `logseq/.recycle`-equivalent / git history) | `Editor.deletePage` |

### 5.4 Conventions

- `content` is Logseq Markdown for **one block** (first line + continuation lines; no leading `- `); multi-block input uses `blocks[]` trees. Server validates with the parser ([[02-markdown-block-syntax]]) and rejects content that would split into multiple blocks or contains property lines that collide with reserved keys (`id`, `collapsed`) unless explicit.
- Page names accept aliases and are case-insensitive; responses return canonical names.
- Errors: MCP tool errors with `isError: true` and a machine code: `NOT_FOUND`, `CONFLICT`, `BLOCK_BUSY`, `BLOCK_IN_CONFLICT`, `READ_ONLY`, `FORBIDDEN_SCOPE`, `INVALID_CONTENT`, `RATE_LIMITED`.
- Annotations: `readOnlyHint` on read tools, `destructiveHint` on delete tools, `idempotentHint` on `set_block_property`.
- Pagination via `cursor` for list tools; outputs capped (~20k chars text) with `truncated: true`.

---

## 6. Resources

| URI | Content |
|---|---|
| `bitacora://graph/<graph>/page/<url-encoded name>` (short `bitacora://page/<name>` for the default graph) | page Markdown (`text/markdown`), raw file content |
| `bitacora://block/<uuid>` | block subtree Markdown |
| `bitacora://journal/<yyyy-mm-dd>` and `bitacora://journal/today` | journal page |
| `bitacora://graph/<graph>/config` | `logseq/config.edn` (read-only) |
| `bitacora://sync/status` | JSON sync status |

- `resources/list` returns recent/favorite pages (paged); `resources/templates/list` advertises the URI templates above.
- `resources/subscribe` supported: page change events (from the file writer/index) emit `notifications/resources/updated` over the SSE stream of the session.
- Assets: `bitacora://asset/<path>` returns blob content (images) for multimodal agents, size-capped.

## 7. Prompts

| Prompt | Args | Purpose |
|---|---|---|
| `daily_review` | `date?` | gathers today's journal, open tasks, scheduled/deadline items → review template |
| `weekly_review` | `week?` | journals of the week + DONE tasks + new pages |
| `summarize_page` | `name` | page tree + backlinks → summary request |
| `capture` | `text` | instructions to file a note into today's journal with tags/links per graph conventions |
| `logseq_syntax` | — | concise guide to Logseq Markdown (blocks, properties, refs, tasks) so agents write valid content |

---

## 8. Configuration (Settings > Agents / MCP)

| Setting | Default |
|---|---|
| `mcp.enabled` | true (read-only) |
| `mcp.port` | 12316 |
| `mcp.allow_writes` | false |
| `mcp.allow_deletes` | false |
| `mcp.confirm_writes` | false |
| `mcp.allowed_origins` | `[]` |
| `mcp.protected_namespaces` | `[]` |
| `mcp.tokens` | one default token; UI shows "copy config snippet" for Claude Desktop/Code (`{"type":"http","url":"http://127.0.0.1:12316/mcp","headers":{"Authorization":"Bearer …"}}`) |

---

## Requirements for Bitacora

- **MUST** serve MCP over Streamable HTTP (not stdio) from the running app, independent of any window being open.
- **MUST** bind to loopback only by default, require a bearer token always, validate `Origin`/`Host`, and send no permissive CORS headers.
- **MUST** store tokens outside the git-synced graph (OS keychain or app config dir).
- **MUST** route all writes through the same outliner op pipeline as the UI, with undo entries, index updates and git commits tagged as agent writes.
- **MUST** provide scopes (read / write / delete) and default to read-only until the user enables writes.
- **MUST** keep an audit log of tool calls with per-entry undo.
- **MUST** implement optimistic concurrency (`expected_version`) and refuse writes to a block currently being edited or in sync conflict.
- **MUST** expose at least: `search`, `get_page`, `get_page_blocks_tree`/`get_block_tree`, `get_block`, `list_pages`, `list_journals`, `get_today_journal`, `backlinks`, `tasks`, `query`, `create_page`, `append_block`, `insert_block`, `update_block`, `move_block`, `remove_block`, `git_sync_status`.
- **SHOULD** mirror Logseq plugin API names/semantics in descriptions (and optional `POST /api` compatibility endpoint).
- **SHOULD** expose pages/blocks/journals as resources with subscriptions, and provide prompts for reviews and syntax.
- **SHOULD** use `rmcp` (official SDK) with `transport-streamable-http-server` on axum, isolated behind an internal module.
- **SHOULD** return both structured JSON and Markdown text in tool results.

## Implementation notes (read side, BIT-US-0017/0018/0019)

- `GraphReader` (crate `bitacora-mcp`) is the synchronous read trait; `IndexGraphReader` implements it over `bitacora-index` (`IndexReader`, `search::search`) plus the graph folder (raw page text, `logseq/config.edn`, `assets/`). `mcp -> index` is an allowed edge, so no adapter outside the crate was needed. Sync status comes from `SyncStatusProvider` (default `DisabledSync`); the app/CLI supplies a real one via `McpServer::start_with_sync`.
- Every tool checks the `read` scope from the `TokenInfo` the guard stores in the request extensions; missing scope yields an `isError` result with `FORBIDDEN_SCOPE` (resources/prompts return a JSON-RPC error).
- Tool results carry `structuredContent` (with an `outputSchema`) and a Markdown text rendering capped at 20k characters. Lists paginate with opaque `cursor`/`next_cursor`; tree tools cap blocks per call (default 500).
- `version` of a block is `blake3(uuid, content)[..16]`; the page `etag` is the first 16 hex characters of the indexed file hash.
- `query` evaluates only `and`, `(task ..)`, `(priority ..)`, `[[page]]` and `"text"` until the DSL compiler (BIT-US-0101) exists; other terms return `NOT_SUPPORTED`, datalog `INVALID_QUERY`.
- Subscriptions use the legacy `resources/subscribe` request (stateful sessions); index events are mapped to page, journal and block URIs and sent as `notifications/resources/updated`. The headless CLI polls `Indexer::reconcile` every 2 s until the watcher is wired in.
- Assets are confined to `assets/` (canonicalised, symlink escapes refused) and capped at 5 MiB.

## Implementation notes (write side, BIT-US-0020/0021/0022/0023)

- **Bridge** (`bridge.rs`, `QueueBridge`): write tools run on core's single-writer `CommandQueue` with `Source::Mcp` (`Cmd` planners and `Request::Commit`); no file is touched outside core (rule 3). A tool call is a *group* of core transactions, all-or-nothing (committed steps are rolled back with their inverse ops when a later step fails). The audit log keeps the group's transactions in memory; undo commits their inverses in reverse order as one step. `bitacora-mcp` gained edges to `bitacora-config` and `bitacora-markdown` (content validation, property edits) in `xtask/src/deps.rs`; the app/CLI only pass a `QueueBridge` in `ServerParts`.
- **Policy** (`policy.rs`, `WritePolicy`): `mcp.allow_writes` / `mcp.allow_deletes` (default off, live-adjustable through `McpServer::policy()`), per-token sliding window (60 write calls/min, `RATE_LIMITED` with `retry_after_ms`; every attempt counts, even refused ones), 200 blocks per call (`INVALID_CONTENT`), protected pages: `mcp.protected_namespaces` (root or child) and `bitacora-agent-readonly:: true` on the page (read from the index and from the loaded page) give `PROTECTED_PAGE`. Order of checks: global toggle (`READ_ONLY`) -> token scope (`FORBIDDEN_SCOPE`) -> rate limit -> validation -> page guards. `tools/list` hides write tools without toggle+scope and delete tools without both toggles and the `delete` scope; calling a hidden tool still enforces the checks.
- **Optimistic concurrency**: block `version` = `blake3(uuid, content)[..16]`, identical for the index reader and the write path; `expected_version` mismatch -> `CONFLICT` with `current {uuid, content, version}`. `BLOCK_BUSY` (+ `retry_after_ms`) comes from the app-supplied `WriteGate` (`McpConfig::gate`), `BLOCK_IN_CONFLICT` from `SyncStatus.conflict_pages` (page granularity). Block lookup: loaded pages first (blocks with `id::`, including blocks agents created), then the index (uuid, document position verified against the text; a stale index is a `CONFLICT`, never a write to the wrong block). Index-generated uuids are only stable until the file is re-parsed (index design section 2.3): agents should use uuids from a fresh read, and blocks they create always get a persisted `id::`. Edited blocks lacking `id::` get their index uuid written lazily.
- **Content rules**: `content` is one block (no leading `- `, no line that would parse as a bullet, no `id::`/`collapsed::`), validated with core's `text_is_representable` and the markdown property reader. Properties are placed after the title line (Logseq layout), before continuation lines.
- **Rename**: core has no title-changing op, so `rename_page` is delete-old + create-new inside one group: the old file goes to `logseq/.recycle/` (recoverable), the new file gets the same blocks (ids and properties preserved) and `[[old]]`, `#[[old]]`, `#old` references in other pages (from the index's linked references) and in the page itself are rewritten; protected referrers are skipped and reported in `details.skipped_pages`. Journals cannot be renamed. Namespace children are not renamed.
- **Undo safety**: the audit entry records a content hash of every touched page right after the call; undo is refused (`UndoError::Changed`) once any of those pages changed, so undo is effectively last-in-first-out per page. Undo data is in memory: after a restart entries are listed but `NotAvailable`.
- **Audit** (`audit.rs`): JSONL `audit.jsonl` in `<data dir>/mcp-audit` (runtime default; `McpConfig::audit_dir`), 10 MiB x 5 files, an `undone` event line instead of rewriting. One record per tool call (reads included), per `/api` call (`api:<method>`) and per authentication failure (`auth`). No note content (non-content arguments verbatim, others as sizes) and no token values. Runtime API for the "Agent activity" view: `Session::agent_activity(&AuditFilter)`, `Session::undo_agent_entry(id)`, `Session::mcp_policy()`. The view itself is an app task (BIT-T-0211).
- **Agent commits**: after a write that changed the graph the MCP layer calls `SyncStatusProvider::note_agent_write(token)`; the runtime forwards it as `Command::AgentWrite` and the engine records its next automatic commit as `Bitacora-Kind: agent` + `Bitacora-Agent: <token name>` (never squashed). `git_sync_now` maps to `SyncCommand::SyncNow`.
- `clientInfo` is only available in stateful sessions; in stateless JSON mode the audit shows the transport default.

### Compatibility API (`POST /api`, BIT-US-0023)

Off by default (`McpConfig::api_enabled`, route absent -> `404`). Same Host/Origin/bearer guard, scopes, toggles, rate limit, protected pages and audit as `/mcp`. Request `{"method": "logseq.Editor.getBlock", "args": [...]}`; errors are `{"error": "CODE: message"}` with HTTP 200 like Logseq; results use camelCase keys and `page: {name}`.

| Logseq method | Args | Maps to |
|---|---|---|
| `Editor.getPage` | `[name]` | `get_page` (`null` when missing) |
| `Editor.getBlock` | `[uuid, {includeChildren}]` | `get_block` |
| `Editor.getPageBlocksTree` | `[name]` | `get_page_blocks_tree` |
| `Editor.getPageLinkedReferences` | `[name]` | `backlinks`: `[[{name}, [blocks]], ...]` |
| `DB.q` | `[dsl]` | `query` |
| `search`, `App.search` | `[text]` | `search` |
| `Editor.insertBlock` | `[uuid or page, content, {before, sibling, properties}]` | `insert_block` (`sibling:false` = last child) or `append`/`prepend_block` for a page name |
| `Editor.appendBlockInPage` / `prependBlockInPage` | `[page, content, {properties}]` | `append_block` / `prepend_block` |
| `Editor.updateBlock` | `[uuid, content, {properties}]` | `update_block` |
| `Editor.moveBlock` | `[uuid, target, {before, children}]` | `move_block` |
| `Editor.removeBlock` | `[uuid]` | `remove_block` (delete scope) |
| `Editor.createPage` | `[name, properties]` | `create_page` with `if_exists: return` |
| `Editor.renamePage` / `deletePage` | `[name, new]` / `[name]` | `rename_page` / `delete_page` (delete scope) |
| `Editor.upsertBlockProperty` / `removeBlockProperty` | `[uuid, key, value]` / `[uuid, key]` | `set_block_property` / `remove_block_property` |

Everything else (`UI.*`, `Git.*`, `App.relaunch|quit`, plugin methods, `DB.datascriptQuery`, unknown names) answers `{"error":"method not supported"}`.

## Implementation notes (token keychain and Settings > Agents, BIT-T-0115 / BIT-T-0116 / BIT-US-0107)

- `TokenStore::load_or_init_with(path, Some(backend))` keeps token **secrets in the OS keychain** (`SecretBackend`; `KeyringBackend`, service `bitacora`, account `mcp/<name>`), and only metadata (name, scopes, `created_at`, `keychain: true`) in `mcp-tokens.json` (version 2, mode 0600). A secret the keychain refuses stays in the 0600 file (warning logged), so a token is never lost; a version 1 file (secrets inline) is migrated on load and a file entry whose secret returns later is migrated at the next load. A keychain that lost a secret shows the token as unavailable (it cannot authenticate; "Replace" repairs it). `TokenStore::load_or_init(path)` stays file-only (tests, `--token-file`); the desktop app and `bitacora-cli serve` with the default token location pass `os_keychain()` through `McpOptions.secrets`.
- `TokenStore::{summaries, set_scopes}` feed the settings page; token changes act on the running server's store, so a revoked token gets `401` on its next request without a restart. "Copy config" builds the `mcpServers` JSON (or the `claude mcp add` command) from `secret_of`, which reads the in-memory copy loaded from the keychain.
- The app persists `mcp.*` (`enabled`, `port`, `allow_writes`, `allow_deletes`, `api_enabled`, `allowed_origins`, `protected_namespaces`, `writes_per_minute`) in `settings.json`. Write toggles and protected namespaces apply live through the server's `WritePolicy`; the rest need the server to restart (the page labels them and offers "Restart the server now", which reopens the graph).
- Tray (BIT-T-0112): GPUI 0.3.8 / GPUI Kit 0.7.1 expose no tray or status-item API, so there is no tray icon. The equivalent is Settings > General > "Keep running in the background" (`keep_running_in_background`): closing the last window keeps the session and the MCP server alive (`/health` keeps answering) and a second launch reopens the window. The Agents page shows the server status, including "port in use". Revisit when GPUI grows a tray API.
- Open: a build without a usable keychain (headless Linux) silently uses the file; cross-OS validation of the real keychain is manual (`cargo test -p bitacora-mcp -- --ignored os_keychain`).

## Open questions

1. Session mode: stateless (`StreamableHttpServerConfig` JSON-response mode) is simpler; stateful sessions are needed for resource subscriptions — enable both? (Resolved: both are implemented, `McpConfig::stateful` selects; subscriptions need stateful.)
2. Should every block touched by an agent get a persistent `id::` (more file churn) or should agents use ephemeral handles valid only for the session?
3. How rich should `query` be — full Logseq simple-query parity, or a documented subset + `sql_query`?
4. "Confirm writes" UX: per call toast vs batched review panel? What happens when the app is in tray with no window?
5. Expose OAuth (MCP auth spec) for remote agents/tunnels in v2, or keep strictly local?
6. Multi-graph: one endpoint with `graph` arg (proposed) vs one endpoint per graph (`/mcp/<graph>`) for simpler per-graph tokens?
