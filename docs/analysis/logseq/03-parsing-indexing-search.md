# 03 — Parsing, indexing, search and queries in Logseq (file-based graph)

> Source: `/www/Bitacora/logseq` (snapshot of 2025-11-14, tag `0.10.15`, commit `03bcefbd`, file-based graph code path, Electron desktop).
> All references are `path:LINE` relative to the Logseq repo root. References prefixed with `master:` point to the DB-graph code on the `master` branch (commit `22a29b30`). Read them with `git show master:<path>`.
> Related: [[01-file-graph-layout]] (directory layout, file naming, config), [[02-markdown-block-syntax]] (block grammar), [[04-editor-outliner-operations]] (writes back to files), [[sqlite-index-schema]] (the Bitacora design based on this analysis).

## Summary

Logseq does not index Markdown as text. It turns every file into **entities in an in-memory Datascript database** (a Datalog triple store) and treats that DB as its working model. The pipeline:

1. **mldoc** (an OCaml parser compiled to JS) parses the file into an AST with byte positions.
2. **graph-parser** (`deps/graph-parser`) walks the AST. It produces one *page* map per file and one *block* map per list item, plus the pre-block (text before the first bullet). Each block gets its raw text, properties, refs, task marker, priority, scheduled and deadline dates, parent, a `left` sibling pointer and inherited **path-refs**.
3. The file's old blocks are retracted, except those whose UUID survives. The new entities are transacted as one Datascript transaction.
4. A tx listener updates the search index, recomputes affected live queries, and persists the whole DB as a transit file in `~/.logseq/graphs/` when the app is idle.

Search does not go through Datascript. On desktop, Logseq already keeps a **SQLite FTS5 index per graph** (`better-sqlite3`, tables `blocks`, `blocks_fts`, `pages`, `pages_fts`, kept in sync by triggers) in Electron's `userData/search`. Page-title search is an in-memory **fuse.js** fuzzy index. The DB-graph version on `master` keeps SQLite but switches to the `trigram` tokenizer and adds an exact-title lookup, a LIKE-based fuzzy pass and rank fusion.

Queries come in two kinds. **Simple queries** (`{{query (and [[a]] (task TODO))}}`) are an EDN s-expression DSL compiled to Datalog plus a fixed set of rules. **Advanced queries** are raw Datascript Datalog with `:inputs`, a `:result-transform` evaluated by SCI, and a hiccup `:view`. Everything in the DSL can be mapped to SQL over a relational schema. Raw Datalog can only be supported as a subset.

Implications for Bitacora:
- Logseq already proves that the search index can be a **disposable SQLite cache**. It deletes and rebuilds it on corruption (`src/electron/electron/search.cljs:34-44`).
- Block identity is **unstable unless `id::` is present**. Logseq papers over this with a 2-way diff on external edits and by writing `id::` into files when a block gets referenced. Bitacora has to choose a policy (see Open questions).
- `path-refs` (refs inherited from ancestors, plus the page itself) are what every backlink and query uses. Bitacora must reproduce them exactly.

---

## 1. Initial graph load

### 1.1 Discovering files

| Step | Behaviour | Ref |
|---|---|---|
| Ignore rules | Ignores paths starting with `.`, plus `logseq/.recycle`, `logseq/bak`, `logseq/version-files`, `logseq/graphs-txid.edn`, `logseq/pages-metadata.edn` and `**/node_modules/` | `deps/common/src/logseq/common/graph.cljs:42-67` |
| `:hidden` config | Drops any path matching a pattern in `config.edn :hidden` | `deps/common/src/logseq/common/config.cljs:18-25` |
| Supported formats | Only `.md`/`.markdown`/`.org` are parsed by mldoc. `.edn` and `.css` are loaded as files. Whiteboards are `.edn` under `whiteboards/` | `deps/graph-parser/src/logseq/graph_parser.cljs:133-148`, `deps/graph-parser/src/logseq/graph_parser/config.cljs:55-60` |
| Parse order | Journals newest first, then built-ins (`contents.*`, `*.edn`, `custom.css`), then the rest, sorted by path | `deps/graph-parser/src/logseq/graph_parser.cljs:142-148` |
| Watcher-based order | `load-graph-files!` sorts `logseq/` first, then `journals/`, then `pages/` | `src/main/frontend/fs/watcher_handler.cljs:207-210` |

### 1.2 Two load paths

**A. New graph / re-index** (`src/main/frontend/handler/repo.cljs:174-248`, `parse-files-and-create-default-files-inner!`):
- A `core.async` go-loop calls `parse-and-load-file!` → `file-handler/alter-file` → `reset-file!` → `graph-parser/parse-file` with `:skip-db-transact? true`, so each call returns tx data without transacting it (`repo.cljs:132-158`).
- Tx data is **batched and transacted every 100 files** (`repo.cljs:240`). Whiteboards are transacted on their own.
- Above 1000 files (`large-graph?`, `repo.cljs:183`) the loop yields to the UI only every 10th file (`repo.cljs:205-212`).
- **Duplicate page titles:** when two files resolve to the same `:block/name`, the second file is **skipped** and the user sees a warning (`repo.cljs:226-236`).
- `:extracted-block-ids` is an atom shared across the whole load, used to detect duplicate `id::` values across files (`repo.cljs:186`).

**B. Re-opening a known graph** (the common case):
1. `restore-graph!` deserializes the whole Datascript DB from `~/.logseq/graphs/<graph>.transit` (`src/main/frontend/db.cljs:157-182`, `src/electron/electron/handler.cljs:275-296`). If `:schema/version` is older, the DB is migrated (`db.cljs:71-88`).
2. `preload-graph-homepage-files!` reparses today's journal (or the home page) first, so the first screen is fresh (`watcher_handler.cljs:139-190`).
3. `load-graph-files!` replaces the old initial chokidar scan (`watcher_handler.cljs:192-262`):
   - It reads the directory, computes `deleted-files = db-files − disk-files` and retracts them in one tx (`:236-240`).
   - For **every** file it reads the content and stat, then calls `handle-changed!` with `"add"` or `"change"` (`:241-253`).
   - `handle-changed!` reparses only when `trim(content) ≠ trim(db :file/content)` (`:86-102`). **The check is on content, not mtime.** Every file is read on every start, but only changed files are parsed again.
   - A notification appears if the change set is larger than 100 files (`:226-233`).

The whole file text is stored in the DB as `:file/content` (`deps/graph-parser/src/logseq/graph_parser.cljs:121-125`). That stored copy is the baseline for change detection and for the "modified on disk" check.

### 1.3 `parse-file`: per-file pipeline

`deps/graph-parser/src/logseq/graph_parser.cljs:71-131`:

```
format ← file extension
{pages, blocks, ast} ← extract/extract(file, content, opts)        ; mldoc + graph-parser
delete-blocks ← delete-blocks-fn(db, first page, file, new uuids)   ; retract old blocks
block-refs-ids ← uuids referenced via ((uuid))                     ; pre-create {:block/uuid}
pages ← with-ref-pages(pages, blocks)                               ; merge + assign squuids
tx = [file] ++ [{:block/name}...] ++ delete-blocks ++ pages ++ block-ids ++ blocks
     ++ [{:file/path :file/content [:file/created-at]}]
d/transact!(conn, tx, {:from-disk? true ...})
```

`get-blocks-to-delete` (`graph_parser.cljs:46-69`) clears blocks from **both** the page the file used to define and the page it defines now, since `title::` can rename the page. Blocks whose UUID appears again in the new parse are only stripped of their attributes (`retract-attributes`, `deps/db/src/logseq/db/schema.cljs:105-126`) instead of retracted. That keeps their entity id, so `((uuid))` refs from other pages stay intact.

---

## 2. Incremental reparse on file changes

### 2.1 Watcher (Electron main)

`src/electron/electron/fs_watcher.cljs`:
- **chokidar** with `ignoreInitial`, `awaitWriteFinish: true` (waits until file size stops changing), no polling, and the same ignore predicate as above (`:61-74`).
- Events: `add`, `change`, `unlink`, plus `addDir`/`unlinkDir` (only for the graph root, to detect a missing or remounted graph) (`:76-100`).
- `unlink` is **delayed 500 ms** and dropped if the file exists again, which absorbs atomic-save rename dances (`:91-97`).
- Each event carries the **full file content and `fs.stat`** to the renderer (`:46-60`).
- An event goes to one window of that graph. `addDir`/`unlinkDir` go to all windows (`:22-44`).

### 2.2 Renderer handler

`src/main/frontend/fs/watcher_handler.cljs:58-137` (`handle-changed!`):

| Event | Condition | Action |
|---|---|---|
| `add` | content ≠ db content (trimmed) | Back up the old db content if any, then `alter-file … {:from-disk? true :fs/event :fs/local-file-change}` (`:86-89`, `:44-56`) |
| `change` | content ≠ db content, not an asset | Same as `add`. **Skipped** for a journal whose content equals the default journal template, `-` or `*`, so freshly created empty journals don't bounce (`:91-102`) |
| `unlink` | file was in DB, graph dir still exists | `page-handler/delete!` on the page linked to that file, with `:delete-file? false` (`:104-110`) |
| `change` to `logseq/custom.css`, `config.edn` | | Reloads CSS or config (`:113-127`) |

After the reparse, `set-missing-block-ids!` scans the new content for `((uuid))` refs. Any referenced block whose file lacks an `id::` property gets **`id:: <uuid>` written into its source file** (`:28-42`). `:file/last-modified-at` is set from the stat mtime (`:56`, `src/main/frontend/db/model.cljs:216-224`), but nothing ever reads it for change detection.

### 2.3 Keeping block UUIDs across external edits (2-way diff)

A block without `id::` gets a fresh random `squuid` on every parse (`deps/graph-parser/src/logseq/graph_parser/block.cljs:424-432`). To keep UUIDs stable when a file is edited outside Logseq, `reset-file!*` injects a `:resolve-uuid-fn` when the event is `:fs/local-file-change` (`src/main/frontend/handler/common/file.cljs:70-93`):

- `diff-merge-uuids-2ways` (`common/file.cljs:57-68`) builds "diff blocks" `{uuid, body, level}` from the DB in outline order (`src/main/frontend/fs/diff_merge.cljs:27-40`) and from the new AST (`diff_merge.cljs:51-92`).
- It runs `Differ.diff_logseqMode` from `@logseq/diff-merge`, then `attach_uuids` (`diff_merge.cljs:14-25`).
- The resulting UUID list is applied positionally to the new blocks, but **only if the counts match** (`deps/graph-parser/src/logseq/graph_parser/extract.cljc:130-143`, `:163-166`).
- A 3-way merge (`three-way-merge`, `diff_merge.cljs:170-196`) also exists. It is used only by sync and git (see [[05-git-and-apis]]).

Duplicate UUIDs are handled too. If an `id::` value already exists on another page, or earlier in the same load, the block gets a new UUID and the `id::` line is removed from its content (`block.cljs:610-644`).

### 2.4 "File modified on disk" conflict on write

When Logseq writes a file (`src/main/frontend/fs/node.cljs:21-65`):
- It reads the disk content first. If the disk content differs (trimmed) from the DB's `:file/content` and the file is not `.edn`, `.css` or `.excalidraw`, it **does not write**. It publishes `:file/not-matched-from-disk` instead (`node.cljs:44-50`), which opens a diff modal so the user can pick a version (`src/main/frontend/handler/events.cljs:374-380`).
- When the write succeeds and the disk content differed (the edn/css case), the old disk content goes to **`logseq/bak/<path-without-ext>/<ISO-timestamp>.Desktop.<ext>`**. Only the 6 newest versions are kept (`src/electron/electron/backup_file.cljs:7-51`).
- Incoming external changes also back up the previous DB content before reparsing (`watcher_handler.cljs:46-50`).
- Case-only renames, which fire no event on macOS, are detected when a page name maps to another file path that differs only by case. The `:file/path` is then rewritten (`common/file.cljs:24-49`).

**There is no mtime or hash comparison anywhere. Change detection is always a trimmed string equality against the stored file content.**

---

## 3. Data model (file-graph schema)

The schema is in `deps/db/src/logseq/db/schema.cljs:7-103` (`version 2`). Pages and blocks share the `:block/*` namespace: *"A page is a special block"* (`schema.cljs:6`).

### 3.1 File entity

| Attr | Meaning | Ref |
|---|---|---|
| `:file/path` | unique, path relative to the graph, NFC-normalized (`gp-util/path-normalize`, `deps/graph-parser/src/logseq/graph_parser/util.cljs:24-28`) | `schema.cljs:95` |
| `:file/content` | full raw text (baseline for diffs) | `schema.cljs:97` |
| `:file/created-at`, `:file/last-modified-at` | ms timestamps (not in the declared schema, written ad hoc) | `graph_parser.cljs:121-125`, `model.cljs:216-224` |

### 3.2 Page entity

Built by `build-page-map` / `page-name->map` (`deps/graph-parser/src/logseq/graph_parser/extract.cljc:105-128`, `deps/graph-parser/src/logseq/graph_parser/block.cljs:287-335`).

| Attr | Semantics | Ref |
|---|---|---|
| `:block/name` | unique key: `lower-case` → strip leading/trailing `/` → NFC (`page-name-sanity-lc`) | `util.cljs:134-165` |
| `:block/original-name` | display name. Precedence: **`title::` property** > **file name** (parsed by `:file/name-format`, `:legacy` = `.` → `/` + URL-decode; `:triple-lowbar` = `___` → `/`) > **first heading** | `extract.cljc:30-63`, `util.cljs:153-160`, `:244-258` |
| `:block/journal?`, `:block/journal-day` | `true` + int `yyyyMMdd` when the **page name** parses with any of `[config :journal/page-title-format, "MMM do, yyyy", "yyyy-MM-dd", "yyyy_MM_dd"]`. The name is then re-rendered in the configured format. This applies to any page name, not only files under `journals/` | `block.cljs:273-284`, `date_time_util.cljs:15-41` |
| `:block/namespace` | ref to the parent page `a/b` for page `a/b/c`. Parents `a` and `a/b` are created as pages too | `block.cljs:303-318`, `extract.cljc:194-199` |
| `:block/alias` | many refs, from `alias::`. Each alias page also gets the reverse alias plus the other aliases | `extract.cljc:65-95` |
| `:block/tags` | many refs, from page `tags::` | `extract.cljc:96-102` |
| `:block/properties` | map of page properties, i.e. the **pre-block's** properties | `extract.cljc:187-193`, `:123-125` |
| `:block/properties-text-values`, `:block/properties-order`, `:block/invalid-properties` | raw texts, key order, rejected keys | `schema.cljs:56-61` |
| `:block/file` | ref to the file entity. **Absent for pages that exist only because something references them** ("placeholder" pages) | `extract.cljc:117-118` |
| `:block/format` | `:markdown` / `:org` | `schema.cljs:27` |
| `:block/type` | `"whiteboard"` for whiteboards | `schema.cljs:15-18` |
| `:block/created-at`, `:block/updated-at` | ms. Set only when the entity is new | `block.cljs:319-322` |

Built-in pages are seeded into every DB: the markers `NOW LATER DOING DONE CANCELED CANCELLED IN-PROGRESS TODO WAIT WAITING`, priorities `A B C`, and `Favorites`, `Contents`, `card` (`deps/db/src/logseq/db/default.cljs:6-23`).

### 3.3 Block entity

Built by `construct-block` and `extract-blocks` (`block.cljs:561-608`, `:646-693`).

| Attr | Semantics | Ref |
|---|---|---|
| `:block/uuid` | `id::` (or `custom_id`/`custom-id`) property if it is a valid UUID, else a new random squuid | `block.cljs:424-432` |
| `:block/content` | raw text of the block from the source, sliced by UTF-8 byte offsets. The bullet marker and level spaces are stripped and continuation-line indentation removed. The content **includes** the marker, priority, property lines, SCHEDULED/DEADLINE lines and child-less body | `block.cljs:406-422`, `deps/graph-parser/src/logseq/graph_parser/text.cljs:61-77` |
| `:block/page` | ref to page | `extract.cljc:183` |
| `:block/parent` | ref to parent block, or to the page for top-level blocks | `block.cljs:695-768` |
| `:block/left` | ref to the previous sibling, or to the parent when the block is the first child (a linked list) | same |
| `:block/level` | outline depth, used during parsing only. Dissoc'ed before transact (`extract.cljc:211`), but some code paths still pull it | |
| `:block/format` | from file extension | `extract.cljc:182` |
| `:block/marker` | `TODO`/`DOING`/`DONE`/`LATER`/`NOW`/`WAIT(ING)`/`CANCEL(L)ED`/`IN-PROGRESS` from mldoc heading | `schema.cljs:50-51` |
| `:block/priority` | `"A"`/`"B"`/`"C"` | `schema.cljs:53-54` |
| `:block/scheduled`, `:block/deadline` | int `yyyyMMdd` (time and repeater are dropped). `:block/repeated?` is set when a repeater is present | `block.cljs:255-271` |
| `:block/properties` | map keyword → parsed value: a **set of page names** if the value contains refs or is a comma-list property, else an int, boolean or trimmed string | `block.cljs:204-238`, `text.cljs:165-187` |
| `:block/properties-text-values`, `:block/properties-order` | raw values and key order | `block.cljs:229-235` |
| `:block/collapsed?` | from `collapsed:: true`. Removed from properties | `block.cljs:587-592` |
| `:block/pre-block?` | true for the chunk before the first bullet (page properties / front matter), unless it carries `heading::` | `block.cljs:515-553` |
| `:block/refs` | pages and blocks referenced (see §4) | `block.cljs:337-378`, `extract.cljc:184` |
| `:block/path-refs` | refs inherited from ancestors, plus own refs, plus the block's page (§4.3) | `block.cljs:447-492`, `extract.cljc:174-185` |
| `:block/tags` | `#tags` captured by mldoc on the heading, also present in refs | `block.cljs:398-404` |
| `:block/macros` | refs to macro entities (`{{name args}}`) | `block.cljs:494-513` |
| `:block/created-at`, `:block/updated-at` | only when the matching properties are integers | `block.cljs:601-607` |
| `:block/title`, `:block/body`, `:block/children`, `:block/meta` | parse-time only. **Dissoc'ed** before transact | `extract.cljc:210-211` |

`properties` is a **map value** on the entity, not one entity per property. Every property query therefore scans `[?b :block/properties ?p]` and applies a `get` predicate (`deps/db/src/logseq/db/rules.cljc:129-138`). That is one of the slowest paths in Logseq, and a relational EAV table fixes it directly.

Markdown headings are normal blocks. `# Title` outside a list becomes a level-1 block with `heading` property = size (`block.cljs:555-583`).

### 3.4 Property parsing rules

From `text.cljs:132-187`, `block.cljs:134-238` and `property.cljs`:
- **Keys:** lower-cased, `_` and spaces → `-`, `custom_id` → `id`. A key must be a valid EDN keyword without `"^(){}` and must not start with `#`. Invalid keys go to `:block/invalid-properties` (`deps/graph-parser/src/logseq/graph_parser/property.cljs:22-28`).
- **Values:**
  - Built-in "unparsed" keys keep the raw string (`property.cljs:110-127`).
  - A quoted value is kept verbatim.
  - Otherwise refs (`[[x]]`, `#x`) come from the inline mldoc AST. For `alias`, `aliases`, `tags` and any key in `:property/separated-by-commas`, the comma-separated plain text also becomes refs (`text.cljs:141-163`).
  - With no refs, `"true"`/`"false"` and integer strings are converted. Anything else stays a string.
- **Property names become page refs.** Every non-built-in key creates a page, unless `:property-pages/enabled? false` or the key is in `:property-pages/excludelist` (`block.cljs:134-150`). Ref-valued properties add their pages to the block's refs (`block.cljs:165-188`).
- **Hidden built-ins** (`id`, `collapsed`, `heading`, `created-at`, `query-*`, `hl-*`, …) never create pages and are hidden in the UI (`property.cljs:68-79`).

---

## 4. References, path-refs, namespaces, aliases, tags

### 4.1 Page refs from block text

`with-page-refs` (`block.cljs:337-372`) pre-walks the block title and body AST. It skips custom `query` nodes and collects:
- `[[page]]` links (`Link` with `Page_ref`), `[label](file:..)` / org `file:` links, and `[[...]]` inside `Search` URLs (`block.cljs:37-84`)
- nested links `[[a [[b]]]]` (`Nested_link`)
- `#tag` and `#[[tag]]` (`Tag`). Tags containing `#`, spaces or newlines are rejected (`util.cljs:61-64`)
- `{{embed [[page]]}}`
- **the task marker and the priority**, which are added as refs: a `TODO` block references page `todo`, and `[#A]` references page `a` (`block.cljs:339`).
- **Namespace expansion:** a ref to `[[a/b/c]]` also adds refs to `a` and `a/b` (`block.cljs:358-369`, `util.cljs:116-125`). Backlinks of `a` therefore include every block that mentions `[[a/b/c]]`.

Each ref becomes `{:block/name … :block/original-name …}`. Unknown pages are created on transact (`extract.cljc:200-209`, `:290-298`).

### 4.2 Block refs

`((uuid))`, `{{embed ((uuid))}}` and `[label](((uuid)))` / `id:` links are collected from the AST and from property values (`block.cljs:86-120`, `:190-202`). They become `[:block/uuid #uuid]` entries in `:block/refs`. The referenced UUID is pre-transacted as `{:block/uuid}` so the ref can resolve before the target file is parsed (`graph_parser.cljs:109-115`). The block-ref regex is `\(\(([0-9a-f]{8}-…-[0-9a-f]{12})\)\)` (`deps/graph-parser/src/logseq/graph_parser/util/block_ref.cljs:9`).

### 4.3 path-refs

**At parse time** (`with-path-refs`, `block.cljs:447-492`), the block list is walked in document order with a stack of ancestors:

```
path-refs(b) = {page(b)} ∪ refs(b) ∪ ⋃ refs(a) for each ancestor a of b
```

The page lookup is consed in by `extract.cljc:174-177`. Block refs are removed (`(remove vector?)`), so path-refs **contain only pages**. The pre-block is a level-1 sibling, so page-property refs such as `tags:: x` are **not** inherited by the other blocks.

**At edit time**, `compute-block-path-refs` (`src/main/frontend/modules/outliner/pipeline.cljs:27-83`) recomputes path-refs for each changed block and its descendants after outliner ops. It skips collapse/expand, delete, undo and redo (`src/main/frontend/db/react.cljs:326-331`), and the whole pass is skipped for `:from-disk?` transactions (`pipeline.cljs:88-90`), where the parser has already computed path-refs.

Since a block's ancestors always live in the same file, **path-refs are file-local**. A file reparse fully determines them. For Bitacora this means they can be computed in the parse worker, or derived at query time from outline intervals.

### 4.4 Namespaces

- `namespace-page?` is true when the name contains `/` and is not a URL, `./` or `../` (`text.cljs:79-85`).
- The page `a/b/c` gets `:block/namespace → a/b`. Page `a/b` gets `→ a`, and so on (`block.cljs:315-318`).
- The hierarchy is the recursive rule `namespace` (`deps/db/src/logseq/db/rules.cljc:7-12`), used by `get-namespace-pages` / `get-namespace-hierarchy` (`model.cljs:1545-1579`).
- `get-page-namespace-routes` resolves a bare `c` to `…/c` pages by suffix scan (`model.cljs:1585-1602`).

### 4.5 Aliases

- `alias::` on page P creates pages for each alias A. P gets `:block/alias A`, and A gets `:block/alias` to P and to the other aliases (`extract.cljc:65-95`).
- The `alias` rule is symmetric and 2-hop (`rules.cljc:14-24`). `page-alias-set` returns `{P} ∪ alias-closure(P)` (`model.cljs:316-330`).
- **Redirect:** navigating to an empty or placeholder alias page goes to the source page, i.e. the page whose `alias` property lists it (`model.cljs:161-184`, `:1046-1069`).
- Linked references of P are the union over the alias set (`model.cljs:1255-1283`).

### 4.6 Tags

- **Page tags:** `tags::` in page properties → `:block/tags`. Each tag is also a page ref of the pre-block. The query rules `page-tags` and `all-page-tags` use it (`rules.cljc:90-98`). `get-tag-pages` lists pages tagged X (`model.cljs:78-89`).
- **Block tags:** `#x` inside a block is a normal ref (and `:block/tags`). In the file graph there is no semantic difference between `#x` and `[[x]]`.

---

## 5. Block ordering and the parent/child tree

### 5.1 Deriving the tree from indentation

`with-parent-and-left` (`block.cljs:695-768`) walks blocks in document order with a stack of `parents` keyed by `level-spaces`, the mldoc indentation level:
- **Same level as the stack top** → sibling: same parent, `left` = previous sibling.
- **Deeper** → child of the stack top: `left` = parent. Any jump depth is clamped to parent level + 1 (`:723-738`), so badly indented children still attach to the nearest shallower block.
- **Shallower** → pop to the matching level and become its sibling. If no ancestor has exactly that level, the block attaches under the closest shallower one and takes the level of the sibling it follows (`:740-766`).
- Top-level blocks have `:block/parent` = the page.

### 5.2 Ordering: `:block/left` linked list (file graph) and `:block/order` (DB graph)

- **File graph:** siblings form a singly linked list through `:block/left`. `sort-by-left` rebuilds the order through a `left-id → block` map, and asserts that no two blocks share a `left` (`model.cljs:380-408`). A page's full outline is a DFS over `:block/_parent` sorted by left (`model.cljs:429-478`). Every move or insert rewrites `left` pointers. The linked list is fragile: duplicates or cycles corrupt the outline, which is why the assert exists.
- **DB graph (`master`):** `:block/order` is a **fractional-indexing string key** (`master:deps/db/src/logseq/db/common/order.cljs:1-40`, `gen-key` / `gen-n-keys` via `logseq.clj-fractional-indexing`). Children are sorted by `(parent, order)` with no chain walking, and inserting between two siblings writes one row.
- **For Bitacora's index:** the Markdown file is the source of truth and it is reparsed as a whole. The index can therefore store plain integer **document order** (`ord`, pre-order position in the file) plus `subtree_end` (the last `ord` of the subtree). That gives O(1) "is descendant of" checks and range scans for subtrees and path-refs. Fractional keys are only needed if the index ever becomes the write model. See [[04-editor-outliner-operations]] for how edits rewrite files.

---

## 6. Backlinks, unlinked references, graph view

| Feature | Implementation | Ref |
|---|---|---|
| **Linked references** of page P | Blocks whose `:block/path-refs` contains any page of `page-alias-set(P)`, **excluding blocks on P itself**, grouped by page. Because of path-refs, children of a referencing block show up too, and the UI collapses them under their top-most matching ancestor | `model.cljs:1227-1283` |
| Reference filters | Stored as page property `filters:: {"foo" true, "bar" false}`, include/exclude by ref page | `src/main/frontend/handler/page.cljs:751-758`, `src/main/frontend/components/reference.cljs:125-202` |
| **Block references** (count/list) | `[?ref-block :block/refs ?block]`, i.e. `:block/_refs` | `model.cljs:1352-1371` |
| **Unlinked references** | Full scan of every `:block/content` datom with a case-insensitive regex per name and alias: `(^\|[^\[#0-9a-zA-Z]\|((^\|[^\[])\[))NAME($\|[^0-9a-zA-Z])`, i.e. a whole-word match not preceded by `[[` or `#`. Blocks on the page itself are excluded. The logbook drawer is removed first. **O(all blocks).** | `model.cljs:1320-1350` |
| Pages that mention P | `[?block :block/refs ?p] [?block :block/page ?m]` | `model.cljs:1203-1225` |
| **Global graph view** | Edges `page → ref-page` from `:block/refs` (**not** path-refs) of every block, optionally excluding journals. Redundant edges to ancestor namespaces are dropped. Namespace edges and tag colouring are added, and orphan, built-in and `exclude-from-graph-view` pages are filtered | `model.cljs:1178-1199`, `src/main/frontend/handler/graph.cljs:84-121` |
| Page graph | refs from P, mentions of P, tags, namespaces, plus n-hop expansion | `graph.cljs:123-177`, `:217` |
| Orphan pages | no `:block/_refs`, empty or only `-`, not built-in, not a whiteboard, not a namespace parent | `model.cljs:1621-1653` |
| Scheduled/deadline panel on journals | blocks with `scheduled`/`deadline` ≤ today + N days, not DONE or CANCELED, repeated or ≥ today | `model.cljs:1285-1318` |

---

## 7. Full-text search

### 7.1 Engines

`frontend.search.agency` dispatches to **Node** (Electron: SQLite via IPC) or **Browser** (fuse.js), plus plugin search services (`src/main/frontend/search/agency.cljs:10-64`).

### 7.2 Electron: SQLite FTS5 (the Bitacora precedent)

`src/electron/electron/search.cljs`:
- **One SQLite file per graph** in `app.getPath("userData")/search/<sanitized-repo>` (`:125-145`).
- Tables (`:96-123`):
  ```sql
  CREATE TABLE blocks (id INTEGER PRIMARY KEY, uuid TEXT NOT NULL, content TEXT NOT NULL, page INTEGER);
  CREATE VIRTUAL TABLE blocks_fts USING fts5(uuid, content, page);          -- default unicode61
  CREATE TABLE pages  (id INTEGER PRIMARY KEY, uuid TEXT NOT NULL, content TEXT NOT NULL);
  CREATE VIRTUAL TABLE pages_fts USING fts5(uuid, content);
  ```
  `id` is the **Datascript entity id**. Insert, update and delete triggers mirror rows into the FTS tables (`:46-94`).
- **Query** (`search-blocks`, `:261-288`):
  1. ` and `/`&`, ` or `/`|` and ` not ` are rewritten to FTS5 `AND`/`OR`/`NOT` (`:244-255`).
  2. Without operators, two MATCH attempts run: the raw query, then the quoted phrase. Results are `ORDER BY rank` (bm25).
  3. A `LIKE '%w1%w2%'` fallback is always appended.
  4. Results are deduplicated by rowid and cut to `limit` (default 20). An optional `page = ?` restricts to one page.
- **Page content search** indexes `"$pfts_f6ld>$ <title> $<pfts_f6ld$ <whole file content>"` per page and uses FTS5 `snippet()` with sentinel markers for highlighting (`:325-353`, `src/main/frontend/search/db.cljs:52-63`).
- **Corruption handling:** if `prepare` fails (for example the vtable constructor fails), the DB is deleted and the renderer is asked to rebuild (`search.cljs:34-44`, `:147-168`).

### 7.3 What gets indexed, and how

`src/main/frontend/search/db.cljs`:
- **Blocks:** `:block/content` with built-in properties removed (`id::`, `collapsed::`, …), then `search-normalize`: lower-case + **NFKC**, plus `removeAccents` when `:feature/enable-search-remove-accents?` is on (`db.cljs:18-35`, `src/main/frontend/util.cljc:1024-1031`). Blocks longer than `:block/content-max-length` (default **10 000** chars) are skipped (`src/main/frontend/state.cljs:680-682`).
- **Page content:** the whole file content, prefixed by the title, skipped above 10× max length (`db.cljs:52-63`).
- **Page titles:** an **in-memory fuse.js** index over normalized original names (`threshold 0.5`, `distance 1024`). Results are post-filtered by `exact-matched?`, a subsequence check (`db.cljs:101-120`, `src/main/frontend/search.cljs:121-157`).
- **Property names and values, templates, non-markdown files:** a hand-written **subsequence fuzzy scorer** that rewards consecutive matches and early positions, adds a length-similarity term and a +1000 bonus for exact substrings (`search.cljs:41-90`, used by `:159-215`).

### 7.4 Incremental updates

`after-transact-pipelines` calls `sync-search-indice!` on every transaction (`src/main/frontend/modules/outliner/datascript.cljc:29-39`). It inspects `tx-data` datoms (`search.cljs:259-342`):
- `:block/content` datoms → blocks to upsert or delete, by entity id
- `:block/name` and `:block/original-name` datoms → page-title fuse index add/remove
- `:file/content` datoms → reindex the content of the owning page

Electron applies deletes before upserts so that the same id can be deleted and re-added (`src/electron/electron/handler.cljs:323-339`). A full **rebuild** truncates and bulk-upserts everything from the DB (`handler.cljs:315-321`).

### 7.5 DB-graph search on `master`: ideas worth borrowing

`master:src/main/frontend/worker/search.cljs`:
- `blocks(id TEXT PK = uuid, title, page)` + `blocks_fts USING fts5(id, title, page, tokenize="trigram")` for **substring matching**, which works for CJK and partial words (`:51-63`). There is also a `NOCASE` index on title for exact-title hits (`:65-67`).
- Triggers key FTS rows by `rowid` because FTS5 can't index `id` (`:21-43`).
- The pipeline (`:991-1068`):
  1. **Exact title match** first (`title = ? COLLATE NOCASE`).
  2. FTS `MATCH`, using a phrase query when the input contains punctuation.
  3. `LIKE` for queries of 2 characters or fewer, since trigram needs at least 3.
  4. A **LIKE subsequence fuzzy pass** (`%a%b%c%`) that fetches 4× the needed candidates (capped at 400), pages first, re-scored with the fuzzy scorer.
  5. Optional vector search.
  6. Merge with **Reciprocal Rank Fusion** (`:738-766`).
- Snippets are produced in application code rather than with FTS5 `snippet()` (`:204-400`).

---

## 8. Query language

### 8.1 Simple query DSL (`{{query …}}`)

`src/main/frontend/db/query_dsl.cljs`. The pipeline is: `pre-transform` (string rewrite) → `cljs.reader/read-string` → `simplify-query` → `build-query` (recursive) → `add-bindings!` → `query-wrapper` → `react-query` (`:451-597`).

**Pre-transform** (`:452-476`):
- `[[x]]` → `"[[x]]"`
- `#x` → `#tag x`, read as a page ref
- `(between -7d +7d)` arguments are turned into keywords
- A query that is only a string (`"foo"`) means full-text search.

**Grammar** (operators are case-insensitive):

```
query    := filter | "string" | [[page]] | #tag
filter   := (and query+) | (or query+) | (not query+)
          | (between START END)                     ; journal-day of the block's page
          | (between created-at|last-modified-at START END)  ; property timestamps (ms)
          | (property KEY) | (property KEY VALUE)    ; block properties, excluding pages
          | (page-property KEY) | (page-property KEY VALUE)
          | (task M+) | (todo M+)                   ; markers, case-insensitive
          | (priority P+)
          | (page NAME)                              ; block is on page NAME
          | (namespace NAME)                         ; page's *direct* namespace parent = NAME
          | (page-tags T+) | (all-page-tags)
          | (sort-by KEY [asc|desc])                 ; post-process, by block property; default desc
          | (sample N)                               ; post-process, random N
START/END := today | yesterday | tomorrow | ±N(d|w|m|y) | [[Journal Title]]  ; +h, +n(min) for timestamps
```

**Semantics** (rules in `deps/db/src/logseq/db/rules.cljc:63-143`):

| DSL | Datalog rule | Notes |
|---|---|---|
| `[[p]]` | `page-ref`: `?b :block/path-refs ?br, ?br :block/name p` | **Uses path-refs**: matches children of referencing blocks, and **every block on page p**, since path-refs include the page |
| `"text"` | `block-content`: `clojure.string/includes?` on `:block/content` | **Case-sensitive substring scan**. Not FTS. There is no `full-text-search` keyword in this version |
| `(task …)` | `?b :block/marker ?m, contains?` | |
| `(priority …)` | `?b :block/priority` | |
| `(property k v)` | `get` on the properties map, then `= v`, or `contains?` for set values, also `(str v)` | excludes pages (`missing? :block/name`). `v` goes through `parse-property-value`: ints and bools, `#x` → x, `[[x]]` → x (`query_dsl.cljs:242-263`) |
| `(page-property k v)` | same on page entities | |
| `(between a b)` | `?b :block/page ?p, ?p :block/journal? true, journal-day in [a,b]` | dates of **journal pages**, not scheduled or deadline |
| `(page p)` | `?b :block/page ?bp, ?bp :block/name p` | |
| `(namespace n)` | `?p :block/namespace ?parent, ?parent :block/name n` | direct children only (non-recursive) |
| `(page-tags …)` | `?p :block/tags ?t, ?t :block/name ∈ tags` | |
| `and/or/not` | Datalog conjunction, `or`, `not` | `not` adds the bindings `[?b :block/uuid]` / `[?p :block/name]` (`:478-503`) |

**Result type:** the query returns blocks if any block-level filter appears (`[[ref]]` outside page filters, string, `between`, `property`, `task`, `priority`, `page`), otherwise pages (`:393-397`, `:552-564`). Block results are pulled with `block-attrs` (`model.cljs:37-64`), annotated with their page, grouped by page in the UI, or shown as a table using `query-properties`, `query-sort-by` and `query-sort-desc`.

### 8.2 Advanced queries

`{:title … :query [:find … :where …] :inputs [...] :rules [...] :result-transform … :view … :collapsed? …}` (`src/main/frontend/db/query_custom.cljs:60-74`, `src/main/frontend/db/query_react.cljs:92-111`):
- A `:query` that is a list rather than a `[:find …]` vector is treated as DSL (`query_custom.cljs:71-73`).
- `(pull ?b [*])` is rewritten to `block-attrs` (`query_custom.cljs:12-23`). DSL rule names used inside `:where` are auto-injected (`:25-58`).
- Backward-compat rewrites: `:page/x` → `:block/x`, `:block/ref-pages` → `:block/refs`, and `(= ?x "[[p]]")` → `contains?` (`query_react.cljs:61-90`).
- **`:inputs` keywords** (`deps/graph-parser/src/logseq/graph_parser/util/db.cljs:76-168`):
  - `:current-page`, `:query-page`, `:current-block`, `:parent-block`
  - `:today`, `:yesterday`, `:tomorrow`, `:right-now-ms`, `:start-of-today-ms`, `:end-of-today-ms`
  - `:±Nd|w|m|y` (journal-day int), `:±Nd-start|end|HHMM…` (ms), `:today-HHMM`, legacy `:Nd-before[-ms]`
  - `"[[page]]"` → lower-cased name
- `:result-transform` is a Clojure fn **evaluated with SCI** (`query_react.cljs:32-59`). `:view` is a hiccup fn, also run through SCI.

Queries commonly found in real graphs touch: `:block/marker`, `:block/priority`, `:block/page`, `:block/name`, `:block/original-name`, `:block/journal?`, `:block/journal-day`, `:block/refs`, `:block/path-refs`, `:block/properties` (with `get`), `:block/content` (with `clojure.string/includes?`), `:block/scheduled`, `:block/deadline`, `:block/parent`, `:block/tags`, `:block/alias`, `:block/namespace`, `:block/created-at`/`:block/updated-at`. Built-ins used include `contains?`, `get`, `get-else`, `missing?`, comparisons, `str`, `re-pattern`/`re-find`, `clojure.string/*`, plus `not`, `not-join`, `or`, `or-join` and rules.

### 8.3 Reactivity

- Every query result is cached in `query-state` under a key (`react.cljs:52-200`). After each tx, `get-affected-queries-keys` derives the keys it touches from the datoms: `::block id`, `::page-blocks`, `::refs page`, and so on (`react.cljs:237-295`).
- **Custom queries re-run after every transaction.** A query that took more than **80 ms** is skipped on refresh, unless the user is editing inside it (`react.cljs:297-324`). While the user is typing, refreshes are queued until input is idle (`react.cljs:379-395`).

---

## 9. Performance tricks

| Trick | Ref |
|---|---|
| The whole DB is serialized as a transit cache (`~/.logseq/graphs/*.transit`) so a restart does not reparse everything. It is persisted 3 s after the last tx while input and DB are idle | `src/main/frontend/db.cljs:90-147` |
| Startup diffs disk against cached `:file/content` and reparses only changed files. Today's journal or the home page goes first | `watcher_handler.cljs:139-262` |
| Initial parse batches 100 files per tx and yields to the UI every 10 files above 1000 files | `repo.cljs:183-247` |
| Lazy page rendering: the first 50 blocks (`initial-blocks-length`), then more in steps (`step-loading-blocks` 25). Pagination walks `next-open-block` and skips collapsed subtrees | `model.cljs:28-30`, `:577-605`, `:786-844` |
| Datom-level invalidation of cached queries. Slow custom queries are not refreshed | `react.cljs:237-324` |
| Search skips blocks over 10 000 chars and page content over 100 000 chars | `search/db.cljs:23-63` |
| Search runs in the Electron main process (SQLite), off the renderer thread | `search/node.cljs:10-38` |
| `(remove-nils)` and `distinct-by` on tx data before transacting, to avoid unique-constraint conflicts | `graph_parser.cljs:109-126` |
| Known slow paths: property queries (map scan), `"text"` DSL (substring scan), unlinked references (regex over all content), the global graph (all refs) | `rules.cljc:108-138`, `model.cljs:1325-1350` |

---

## 10. Requirements for Bitacora

1. **MUST** treat Markdown files as the only source of truth. The SQLite index is a cache that can be deleted at any time and rebuilt from the graph directory with identical results (Logseq already deletes and rebuilds its search DB, `search.cljs:34-44`).
2. **MUST** apply Logseq's ignore rules (dot-paths, `logseq/bak`, `logseq/.recycle`, `logseq/version-files`, `node_modules`, `graphs-txid.edn`, `pages-metadata.edn`) and the `:hidden` config when scanning and watching.
3. **MUST** derive page names exactly like Logseq: `title::` > file name (respecting `:file/name-format` `:legacy` vs `:triple-lowbar`) > first heading. Normalize to `lower-case` + NFC with boundary slashes stripped for the unique key, and keep the original name for display.
4. **MUST** detect journals by parsing the page name with `[:journal/page-title-format, "MMM do, yyyy", "yyyy-MM-dd", "yyyy_MM_dd"]` and store `journal_day` as `yyyyMMdd` int.
5. **MUST** build the block tree from indentation with Logseq's tolerant algorithm (`with-parent-and-left`), including the pre-block, heading-only blocks, and clamping of irregular indentation.
6. **MUST** compute `refs` exactly like Logseq: `[[x]]`, `#x`, `#[[x]]`, nested links, `{{embed}}`, ref-valued properties, **property names** (unless disabled), the **task marker and priority as page refs**, and **namespace parents of referenced pages**. Block refs come from `((uuid))`.
7. **MUST** compute `path-refs = {page} ∪ own refs ∪ ancestors' refs` (pages only) and use them for `[[x]]` queries and linked references.
8. **MUST** treat aliases as a symmetric 2-hop closure for backlinks, and redirect empty alias pages to their source page.
9. **MUST** parse property values with Logseq's rules (refs set / int / bool / string, comma-separated keys from config, quoted strings verbatim) and keep the raw text for display and round-tripping.
10. **MUST** support the complete simple-query DSL in §8.1, with the same result-type rule (blocks vs pages), the same `[[x]]`-via-path-refs semantics and the same date keywords.
11. **MUST** reconcile at startup by diffing the disk against the index, and reparse only changed files. Use `(size, mtime)` as a fast filter and a content hash as the authority, never content equality alone.
12. **MUST** debounce watcher events (`awaitWriteFinish`-like). Delay `unlink` and re-check existence to survive atomic saves. Recognize the app's own writes (expected-hash registry) so they don't trigger conflicts.
13. **MUST** refuse to overwrite a file whose on-disk hash differs from the last indexed hash, and offer a diff (Logseq's `:file/not-matched-from-disk`). Back up to `logseq/bak/…/<ts>.Desktop.<ext>` with the same retention, for Logseq compatibility.
14. **MUST** surface duplicate page titles across files and duplicate `id::` values as diagnostics. Logseq silently skips the second file or rewrites the id; Bitacora should at least report it.
15. **SHOULD** keep block UUIDs stable across external edits when `id::` is absent, using a diff-based carry-over like `diff-merge-uuids-2ways`. Write `id::` to the file only when the block is referenced or bookmarked, as Logseq does in `set-missing-block-ids!`.
16. **SHOULD** use two FTS5 indexes: word-level (`unicode61 remove_diacritics 2`, bm25 ranking) and `trigram` for substrings and CJK. Run exact-title, FTS, LIKE fallback for queries under 3 chars and a subsequence-fuzzy pass, then merge with RRF, as on `master`.
17. **SHOULD** index the block text with built-in properties removed, NFKC + lower-case, optionally accent-folded. Skip or truncate blocks over a configurable max length (default 10 000).
18. **SHOULD** support a documented subset of advanced Datalog queries (§8.2), compiled to SQL, and give a clear "unsupported" message for `:result-transform` and `:view` until a scripting story exists.
19. **SHOULD** implement unlinked references with an FTS phrase prefilter followed by Logseq's regex, not a full scan.
20. **SHOULD** compute the graph view from direct `refs` (not path-refs), with namespace edges and filters for journals, orphans, built-ins and `exclude-from-graph-view`.
21. **SHOULD** lazy-load page outlines (first about 50 blocks, then pages) and skip collapsed subtrees.
22. **MAY** borrow fractional ordering keys (`:block/order`) only if the index ever becomes a write model. Pure document order is enough for a reparse-per-file cache.

## 11. Open questions

1. **Block identity without `id::`:** should Bitacora (a) accept unstable IDs, (b) carry IDs over by diff like Logseq, or (c) eagerly write `id::` (noisy diffs in git)? Bookmarks, UI state and cross-file refs depend on this.
2. **Org-mode:** Logseq file graphs can contain `.org` pages. Is Org parsing in scope for v1, or are such files indexed as opaque text?
3. **Parser fidelity:** mldoc has many edge cases (drawers, logbook, src blocks with bullets, front matter). Should we port mldoc behaviour test by test, using Logseq's `deps/graph-parser/test` fixtures as a conformance suite?
4. **`[[x]]` matches every block on page x** because path-refs include the page. Do we keep this surprising behaviour for compatibility, or offer a "strict" mode?
5. **Advanced queries:** which Datalog subset (attributes, predicates, rules, `or-join`/`not-join`) do real user graphs need? Should we sample public graphs to decide? Is an embedded scripting language for `:result-transform` desirable?
6. **Property pages:** Logseq creates a page for every property key. Should they appear in "All pages" and graph view by default, given that `:property-pages/enabled?` is user-configurable?
7. **Duplicate page title** between two files: which file wins in Bitacora? Logseq keeps the first one parsed, which depends on load order.
8. **Whiteboards** (`.edn` + tldraw): index only their text shapes for search, or ignore them?
9. **Config-dependent parsing** (date format, filename format, comma-separated properties, property pages): a config change should invalidate the whole index. Confirm that a "config hash" in index metadata is acceptable.
