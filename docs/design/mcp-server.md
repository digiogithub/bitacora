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

## Open questions

1. Session mode: stateless (`StreamableHttpServerConfig` JSON-response mode) is simpler; stateful sessions are needed for resource subscriptions — enable both?
2. Should every block touched by an agent get a persistent `id::` (more file churn) or should agents use ephemeral handles valid only for the session?
3. How rich should `query` be — full Logseq simple-query parity, or a documented subset + `sql_query`?
4. "Confirm writes" UX: per call toast vs batched review panel? What happens when the app is in tray with no window?
5. Expose OAuth (MCP auth spec) for remote agents/tunnels in v2, or keep strictly local?
6. Multi-graph: one endpoint with `graph` arg (proposed) vs one endpoint per graph (`/mcp/<graph>`) for simpler per-graph tokens?
