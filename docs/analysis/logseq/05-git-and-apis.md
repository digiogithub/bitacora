# 05 — Logseq git integration and external APIs

> Source: `/www/Bitacora/logseq` (snapshot of 2025-11-14, commit `03bcefbd`, file-based graph code path, Electron desktop).
> All references are `path:LINE` relative to the Logseq repo root.

## Summary

Logseq's git integration is a **local-only auto-committer**: on a timer (default 60 s) it runs `git add ./*` + `git commit -m "Auto saved by Logseq"` for every open graph. It never fetches, pulls, merges or pushes; there is no conflict detection and no handling of `<<<<<<<` markers (they would be parsed as ordinary block text). Git history is surfaced only as a per-page "history" modal (`git log -p` + `git show` + whole-file "Revert") and a raw "run git command" shell. The only real merge machinery in the codebase belongs to the (now separate, paid) **Logseq Sync** feature: a block-level 3-way merge (`@logseq/diff-merge`) driven by `logseq/version-files/{base,incoming}` snapshots.

Logseq's external API is the **plugin API** (`logseq.App / Editor / DB / Git / UI / Assets`) implemented in `src/main/logseq/api.cljs`, typed in `libs/src/LSPlugin.ts`. The desktop app optionally exposes the *same* API over HTTP (`POST /api` with `{method, args}`) through a Fastify server on `127.0.0.1:12315`, which proxies every call over IPC into the renderer. There is **no MCP server** anywhere in the repo (`grep -riw mcp` over `src libs deps docs packages` finds nothing).

For Bitacora this means: (a) git sync with pull/merge/push is green-field — Logseq gives us only the commit-cadence precedent and a cautionary block-merge precedent; (b) the plugin API names are the vocabulary agents and power users already know, so [[mcp-server]] should mirror them.

---

## 1. Git integration (Electron main process)

### 1.1 Implementation: `src/electron/electron/git.cljs`

| Concern | Behaviour | Ref |
|---|---|---|
| Git binary | `dugite`'s `GitProcess` (bundled git, no system git needed) | `src/electron/electron/git.cljs:2`, `:27` |
| Where the repo lives | If the graph has no `.git`, `git init --separate-git-dir=~/.logseq/git/<graph-path-with-/→_ and :→comma>/.git`; the graph folder only gets a `.git` *file* (gitdir pointer) | `git.cljs:15-22`, `:69-82` |
| Stale gitdir cleanup | Deletes a `.git` file pointing into `.logseq/` whose target vanished | `git.cljs:50-67` |
| Windows | `core.safecrlf false` after init | `git.cljs:81-82` |
| Staging | `git add --ignore-errors ./*`; swallows "permission denied" and `index.lock` errors | `git.cljs:84-92` |
| Commit | `git config core.quotepath false` then `git commit -m <msg>` | `git.cljs:96-100` |
| Default message | `"Auto saved by Logseq"` (manual commits use user text) | `git.cljs:104-106` |
| Author unknown | Detects `Author identity unknown` and opens a "set username/email" modal that writes **`--global`** git config | `git.cljs:115-116`; `src/main/frontend/components/git.cljs:11-46`; `src/main/frontend/handler/shell.cljs:75-82` |
| Error UX | Every other failure → error toast suggesting to disable the feature | `git.cljs:117-118` |
| Scope | Commits **all graphs open in any window**, not just the active one | `git.cljs:120-124`; `src/electron/electron/state.cljs:44-47` |
| Timer | `setInterval` every `:git/auto-commit-seconds` (fallback 6000 ms if not an int); also fires once 100 ms after (re)configuration | `git.cljs:175-193` |
| On close | `before-graph-close-hook!` commits if auto-commit **and** `:git/commit-on-close?` | `git.cljs:195-199`; `src/electron/electron/core.cljs:313` |
| Startup | `configure-auto-commit!` at app ready | `src/electron/electron/core.cljs:294` |
| Raw commands | `raw!` runs arbitrary git args (string split honouring quotes); auto `add` before `commit` | `git.cljs:139-173` |
| Status | `git status --porcelain` | `git.cljs:126-128` |

### 1.2 Configuration keys

These live in the **app-level** Electron config (`<userData>/configs.edn`, `src/electron/electron/configs.cljs:8-11`), not in the graph's `logseq/config.edn`:

| Key | Default | Ref |
|---|---|---|
| `:git/disable-auto-commit?` | `true` (negative logic "for backward compatibility"; auto-commit is **off** by default) | `src/electron/electron/state.cljs:30-33`; `src/main/frontend/state.cljs:1934-1936` |
| `:git/auto-commit-seconds` | `60`, UI accepts 1..86400 | `src/electron/electron/state.cljs:26-28`; `src/main/frontend/components/settings.cljs:272-290` |
| `:git/commit-on-close?` | `false` | `src/electron/electron/state.cljs:35-37`; `settings.cljs:257-270` |
| `:git-auto-push` (graph config) | toggle exists in Settings > Editor but **nothing consumes it** — vestige of the old GitHub-backed web version | `src/main/frontend/state.cljs:635-637`; `settings.cljs:563-570`, `:758` |

Settings panel "Version control": `src/main/frontend/components/settings.cljs:760-779`, tab registered at `:1167`, `:1212`. Changing any value IPCs `:setGitAutoCommit`, debounced 5 s in main (`src/electron/electron/handler.cljs:504-507`).

### 1.3 IPC surface (main ↔ renderer)

`src/electron/electron/handler.cljs`: `:runGit` (`:471-473`, via `raw!`), `:runGitWithinCurrentGraph` (`:475-478`, returns full result object), `:gitCommitAll` (`:498-499`), `:gitStatus` (`:501-502`), `:setGitAutoCommit` (`:505-507`), `:backupDbFile` (`:75`), `:addVersionFile` (`:82-83`).

### 1.4 Frontend UI around git

- **Commit modal** (`mod+g c`, `src/main/frontend/modules/shortcut/config.cljs:542-544`): shows `git status --porcelain` coloured by first char, single-line message input → `gitCommitAll` (`src/main/frontend/components/commit.cljs:14-93`).
- **Page history** (page menu, `src/main/frontend/components/page_menu.cljs:95-104`): `git log -100 --pretty=format:Commit: %h$$$%s$$$%ad -p <file>` then `git show <hash>:<path>` (`src/main/frontend/handler/shell.cljs:64-72`). The selector shows raw text in a `<pre>` and a **whole-file "Revert"** button that overwrites the file (`src/main/frontend/components/git.cljs:48-78`). No diff view, no block-level restore.
- **Command shell** (`src/main/frontend/components/shell.cljs:22-46`; `src/main/frontend/handler/shell.cljs:39-61`): free-form command; `git …` goes to `:runGit`, anything else to `:runCli`; a token denylist (`rm mv rename dd > command sudo`) is the only guard.
- **Disk-vs-DB conflict modal** (not git-specific): before writing, the renderer re-reads the file; if disk content ≠ last known DB content (except `.edn/.css/.excalidraw` and `.recycle`) it publishes `:file/not-matched-from-disk` (`src/main/frontend/fs/node.cljs:40-50`) and shows a modal with a **character-level diff** and two editable textareas "On disk" / "In Logseq", each with "Select this" which rewrites the whole file (`src/main/frontend/handler/events.cljs:374-380`; `src/main/frontend/components/diff.cljs:27-92`). This is the closest thing Logseq has to a "visual conflict UI".

### 1.5 External changes (what happens after a `git pull` done outside Logseq)

The chokidar watcher (`src/electron/electron/fs_watcher.cljs`) sends events to `src/main/frontend/fs/watcher_handler.cljs:58-137`:

- `add`/`change` with different trimmed content → backs up the **previous DB content** to `logseq/bak/<path>/<ISO-ts>.Desktop.md` (keeps the 6 most recent, `src/electron/electron/backup_file.cljs:7-51`) then re-parses the file (`watcher_handler.cljs:44-56`, `:86-102`).
- Re-parsing uses a **2-way block diff** against the DB to keep block UUIDs stable for blocks without `id::` (`src/main/frontend/handler/common/file.cljs:57-68`, `:70-88`, event `:fs/local-file-change`).
- `unlink` → page deleted from DB (`watcher_handler.cljs:104-110`).
- Journal files whose new content equals the default template, `-` or `*` are ignored (`watcher_handler.cljs:95-101`) — a guard against two devices each creating an empty today journal.
- After reload, block refs to blocks lacking `id::` get the property written back (`watcher_handler.cljs:28-42`) → **extra file writes right after a pull**, which create new diffs to commit.

Conflict markers from an external merge are therefore just text: `<<<<<<< HEAD` lines get parsed into block content.

### 1.6 Ignored paths

Graph scanning ignores dot-paths, `logseq/.recycle`, `logseq/bak`, `logseq/version-files`, `logseq/graphs-txid.edn`, `logseq/pages-metadata.edn`, `node_modules`, `.DS_Store` (`deps/common/src/logseq/common/graph.cljs:45-70`). Logseq Sync additionally ignores `~` backup files and user `:file-sync/ignore-files` (`src/main/frontend/fs/sync.cljs:174-192`). Logseq **does not write a default `.gitignore`**; plugins can read/write it via `logseq.Git.loadIgnoreFile/saveIgnoreFile` (`src/main/logseq/sdk/git.cljs:14-28`).

### 1.7 The block-level 3-way merge precedent (Logseq Sync)

`src/main/frontend/fs/diff_merge.cljs` wraps the npm package `@logseq/diff-merge` 0.2.2 (`package.json:100`):

- Blocks are flattened into `{uuid, body, level, meta.raw-body}` from the mldoc AST; `uuid` comes **only** from an explicit `id::` property (`diff_merge.cljs:103-154`). Page-level properties become a pre-block.
- `three-way-merge base income current` runs `Merger.mergeBlocks(base, [current, income])` and rebuilds the file by concatenating `raw-body` of equal/insert ops (`diff_merge.cljs:156-196`).
- Used by Sync when downloading remote files: base snapshot in `logseq/version-files/base/<path>`, remote in `logseq/version-files/incoming/<path>`; if base == current → fast-forward copy, otherwise 3-way merge; if no base → merge against empty string (`src/main/frontend/fs/sync.cljs:1606-1670`; deletion case `:1581-1604`).
- Tests show the semantics (`src/test/frontend/fs/diff_merge_test.cljs:467-487`): property additions from the remote are kept; but **an indentation change on the incoming side is silently dropped** when the local side appends a block (third case) — the merge is not symmetric and never surfaces a conflict to the user. There is no "conflict" output at all: the merge always produces text.

### 1.8 Known shortcomings (observed in code)

1. Local commits only — no fetch/pull/push, no remote config, `:git-auto-push` is dead UI.
2. Fixed-interval commits regardless of activity → noisy history of "Auto saved by Logseq", commits in the middle of typing (whatever was flushed to disk).
3. Separate git dir under `~/.logseq/git/…` surprises users (graph folder is not a self-contained repo; moving the folder breaks it; path-mangling can collide).
4. Commits every open graph, not just the focused one.
5. Writes `user.name/email` with `--global`, polluting the user's global git config.
6. No conflict detection or resolution; merge markers become content; an external pull triggers bulk re-parse, `bak/` backups and follow-up writes.
7. History UI is whole-file, raw text, no diff, no block-level restore.
8. `exec_git_command` / raw shell accept arbitrary git args (e.g. `-c core.sshCommand=…`, `--upload-pack=…`) → command execution vector, reachable from plugins and the HTTP API (see §2.3).
9. Logseq Sync's block merge silently picks a side; never shows the user a conflict.

---

## 2. External APIs

### 2.1 Plugin API (in-renderer)

Implementation: `src/main/logseq/api.cljs` (exports, `^:export` snake_case), `src/main/logseq/api/block.cljs`, `src/main/logseq/sdk/{git,ui,assets,utils}.cljs`. Typings: `libs/src/LSPlugin.ts` (`IAppProxy :348`, `IEditorProxy :553`, `IDBProxy :819`, `IGitProxy :861`, `IUIProxy :883`, `IAssetsProxy :899`, root `ILSPluginUser :951-1107`). Entities: `BlockEntity :180-205` (`id, uuid, left, parent, page, content, properties, children, level, format, marker…`), `PageEntity :207-225` (`name, originalName, journalDay, namespace, file, properties…`), `IBatchBlock :128-132` (`content, properties, children`).

**Editor (most used by integrations)** — TS `libs/src/LSPlugin.ts:553-817`, impl `api.cljs`:

| camelCase (TS) | Signature (abridged) | Impl |
|---|---|---|
| `getPage` | `(pageNameOrId, {includeChildren?})` → `PageEntity` | `api.cljs:542-548` |
| `getAllPages` | `(repo?)` | `api.cljs:550-553` |
| `createPage` | `(name, properties?, {redirect, createFirstBlock, format, journal})` | `api.cljs:555-572`; TS `:738-747` |
| `deletePage` / `renamePage` | `(name)` / `(old, new)` | `api.cljs:574-579` |
| `getCurrentPage` | `()` | `api.cljs:536-540` |
| `getBlock` | `(uuidOrId, {includeChildren?})` | `api/block.cljs:11-28`; TS `:713-716` |
| `getCurrentBlock`, `getSelectedBlocks` | | `api.cljs:704-712`, `:528-534` |
| `getPreviousSiblingBlock` / `getNextSiblingBlock` | `(block)` | `api.cljs:714-727` |
| `getCurrentPageBlocksTree` / `getPageBlocksTree` | `(page)` → nested `BlockEntity[]` | `api.cljs:760-775` |
| `getPageLinkedReferences` | `(page)` → backlinks grouped by page | `api.cljs:777-785` |
| `getPagesFromNamespace` / `getPagesTreeFromNamespace` | `(ns)` | `api.cljs:787-812` |
| `insertBlock` | `(target, content, {before, sibling, isPageBlock, focus, customUUID, properties})` | `api.cljs:603-647`; TS `:681-692` |
| `insertBatchBlock` | `(target, IBatchBlock[], {before, sibling, keepUUID})` | `api.cljs:649-666` |
| `prependBlockInPage` / `appendBlockInPage` | `(page, content, {properties})` | `api.cljs:814-846` |
| `updateBlock` | `(block, content, {properties})` | `api.cljs:676-684` |
| `removeBlock` | `(block)` | `api.cljs:668-674` |
| `moveBlock` | `(src, target, {before, children})` | `api.cljs:686-700`; TS `:773-777` |
| `setBlockCollapsed` | `(uuid, bool\|'toggle')` | `api.cljs:728-740` |
| `upsertBlockProperty` / `removeBlockProperty` / `getBlockProperty` / `getBlockProperties` | | `api.cljs:742-758` |
| `newBlockUUID`, `editBlock`, `selectBlock`, `checkEditing`, `insertAtEditingCursor`, `getEditingBlockContent`, `exitEditingMode`, `openInRightSidebar`… (UI-bound) | | `api.cljs:495-600` |
| `registerSlashCommand`, `registerBlockContextMenuItem` … (UI extension) | | `api.cljs:342-410` |

**DB** — `q(dsl)` simple-query DSL string (`api.cljs:859-864`, uses `frontend.db.query-dsl`), `datascriptQuery(query, ...inputs)` raw Datalog (`api.cljs:866-882`), `custom_query` (`api.cljs:884-889`), hooks `onChanged`, `onBlockChanged` (TS `:819-858`).

**App** — `getInfo`, `getUserConfigs`, `getCurrentGraph`, `getCurrentGraphConfigs/setCurrentGraphConfigs`, `getCurrentGraphFavorites/Recent/Templates`, templates (`getTemplate/existTemplate/createTemplate/removeTemplate/insertTemplate`, `api.cljs:966-998`), `pushState/replaceState` (routing), `registerCommand*`, `execGitCommand` (`api.cljs:907-911`), hooks `onTodayJournalCreated`, `onRouteChanged`… (TS `:348-550`).

**Search** — `search(q)` → full-text block/page search through the active search engine (`api.cljs:1000-1003`); plugins can also *provide* a search engine (`registerSearchService`, TS `:317-345`).

**Git** — `execCommand(args)`, `loadIgnoreFile()`, `saveIgnoreFile(content)` (TS `:861-871`; `src/main/logseq/sdk/git.cljs:9-28`).

**Assets / UI** — `listFilesOfCurrentGraph`, `makeUrl`, `makeSandboxStorage`; `showMsg`, `queryElementRect` … (TS `:883-934`).

### 2.2 HTTP API server (`src/electron/electron/server.cljs`)

- Fastify on `:server/host` (default `127.0.0.1`) / `:server/port` (default `12315`), configured in app config with `:server/tokens` and `:server/autostart` (`server.cljs:19-32`). UI: `src/main/frontend/components/server.cljs`.
- Routes: `GET /` serves `resources/docs/api_server.html` (usage doc); **`POST /api`** with JSON `{"method": "logseq.Editor.getBlock", "args": [...]}` (`server.cljs:140-151`).
- Method resolution: `logseq.<NS>.<camelName>` → `snake_case` of the last segment, prefixed with `ui_` / `git_` / `assets_` for those namespaces (`server.cljs:58-72`). Bare names are passed through.
- Execution: main process forwards via IPC `invokeLogseqAPI` to the **renderer** and waits for a one-shot reply `::sync!<id>` (`server.cljs:96-104`); renderer looks up `window.logseq.api[method]` and applies args (`src/main/electron/listener.cljs:171-186`). Errors return `{error: msg}` with HTTP 200. Request timeout 42 s (`server.cljs:135`).
- Auth: `Authorization: Bearer <token>` checked against `:server/tokens` (`server.cljs:74-94`).

### 2.3 HTTP API weaknesses relevant to Bitacora

1. **No tokens configured ⇒ no auth**: `validate-auth-token` only throws when a token list exists (`server.cljs:77-81`).
2. **CORS `origin: "*"`** (`server.cljs:138`) — combined with (1), any web page in the user's browser can drive the graph (DNS-rebinding / CSRF-style exposure).
3. Exposes everything, including UI-only calls, `exec_git_command`, `relaunch`, `quit`, plugin install — no read-only scope.
4. Works only while a renderer window is alive and a graph is open (`get-current-repo`); single implicit "current graph".
5. Ad-hoc RPC, not MCP; no schemas, no discovery beyond the HTML page; JS-object results with kebab→camel normalisation (`sdk-utils/normalize-keyword-for-json`).

### 2.4 MCP

No MCP server, client or reference exists in the repo. Community MCP servers for Logseq wrap exactly the HTTP API above (`logseq.Editor.*` via `POST /api`), which is why its method names are the de-facto vocabulary.

---

## Requirements for Bitacora

- **MUST** implement real sync (fetch → merge → push) instead of local-only commits; see [[git-sync-merge]].
- **MUST** commit on idle (debounced) rather than a fixed interval, with meaningful, machine-parseable commit messages.
- **MUST** keep `.git` inside the graph folder (standard repo) — never a hidden separate git dir — and **MUST NOT** write global git config; set identity per-repo.
- **MUST** never leave `<<<<<<<` markers in graph files and must detect them if introduced by external tools (treat as conflict, not content).
- **MUST** offer a block-level, per-conflict visual merge UI (accept ours/theirs/both/edit), improving on the whole-file textarea modal of `diff.cljs`; see [[block-editor]].
- **MUST** treat its own post-pull side-effect writes (e.g. adding `id::` to referenced blocks, cf. `watcher_handler.cljs:28-42`) as part of the merge commit, not a new dirty state loop.
- **MUST** ship an MCP server over Streamable HTTP with mandatory token auth, Origin validation and localhost binding; never "no token ⇒ open"; see [[mcp-server]].
- **MUST NOT** expose arbitrary git/shell execution to agents.
- **SHOULD** mirror Logseq plugin API names (`getPage`, `getBlock`, `getPageBlocksTree`, `insertBlock`, `appendBlockInPage`, `updateBlock`, `moveBlock`, `removeBlock`, `q`, `search`, `getPageLinkedReferences`, `createPage`) as MCP tool names/aliases.
- **SHOULD** optionally provide a compatibility `POST /api` endpoint accepting `{method, args}` for existing scripts targeting Logseq's HTTP API.
- **SHOULD** keep Logseq's ignored paths (`logseq/bak`, `logseq/.recycle`, `logseq/version-files`, `graphs-txid.edn`, `pages-metadata.edn`) and generate a default `.gitignore`.
- **SHOULD** keep the "ignore journal file whose content is only the default template / `-`" heuristic for concurrent journal creation.
- **SHOULD** use `id::` as primary block identity but also structural matching (Logseq's 2-way diff) for blocks without ids; see [[02-markdown-block-syntax]], [[04-editor-outliner-operations]].
- **SHOULD** provide per-page history with a block-level diff and selective restore, backed by git.

## Open questions

1. Should Bitacora honour Logseq's `:git/*` keys from an imported Logseq install, or only its own settings?
2. Do we need a Logseq-compatible `POST /api` endpoint at all, or is MCP + a small REST surface enough?
3. Should Bitacora still write `logseq/bak/` backups (Logseq users expect them) or rely on git history only?
4. How should we treat graphs already using Logseq's separate gitdir (`~/.logseq/git/...`) — migrate into `.git/` automatically?
5. Is Logseq Sync's `logseq/version-files/` layout worth reusing as a base-snapshot store, or is the git merge-base sufficient (it is, when everything is committed)?
