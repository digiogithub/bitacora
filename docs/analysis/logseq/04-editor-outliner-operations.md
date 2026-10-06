# 04 — Logseq editor & outliner operations (file-based graphs)

> Source analysed: `/www/Bitacora/logseq`, working tree checked out at tag **0.10.15** (detached HEAD, the last
> file-graph-only release line). In this version the outliner lives in `src/main/frontend/modules/outliner/*`
> (there is no `deps/outliner` yet) and everything below is the Markdown/Org *file graph* code path.
> All references are `path:LINE` relative to the Logseq repo root.
>
> Related: [[01-file-graph-layout]] · [[02-markdown-block-syntax]] · [[03-parsing-indexing-search]] · design: [[block-editor]]

## Summary

Logseq's file-graph editor is a **DB-first outliner with a file projection**:

1. Exactly **one block is edited at a time**, as raw text in a `<textarea>`. Hidden built-in properties (`id::`,
   `collapsed::`, boolean `heading::`, timestamps, …) and the `:LOGBOOK:` drawer are stripped before the text is put in
   the textarea and re-injected when it is saved (`src/main/frontend/handler/editor/property.cljs:67`,
   `src/main/frontend/handler/editor.cljs:298`).
2. Every change (typing auto-save, Enter, Tab, drag & drop, paste, …) becomes an **outliner transaction** of
   DataScript datoms built by a handful of tree primitives: `save-block`, `insert-blocks`, `delete-blocks`,
   `move-blocks`, `move-blocks-up-down`, `indent-outdent-blocks`
   (`src/main/frontend/modules/outliner/core.cljs:875-897`). Siblings are a linked list via `:block/left` +
   `:block/parent`.
3. After every transaction, the **affected pages are queued** on a channel, batched for **1 s**
   (`src/main/frontend/modules/outliner/file.cljs:17`, `:101`) and each page is **re-serialized in full** from the DB
   tree (`src/main/frontend/modules/file/core.cljs:108`) and written to disk. Untouched text is *not* preserved
   byte-for-byte: indentation, bullets, blank lines and trailing whitespace are normalized.
4. Before writing, the current disk content is read and compared (trimmed) with the last content Logseq knows; on
   mismatch the write is **aborted** and a "File … has been modified on the disk" diff dialog is shown
   (`src/main/frontend/fs/node.cljs:44-50`, `src/main/frontend/components/diff.cljs:27`). Writes are plain
   `fs.writeFileSync` (not atomic) with versioned backups under `logseq/bak/`.
5. Undo/redo is **datom-level**: each outliner transaction's `tx-data` is pushed on a per-graph stack and inverted on
   undo (`src/main/frontend/modules/editor/undo_redo.cljs:113-123`, `:247-272`).

For Bitacora (see [[block-editor]]) the key lessons are: keep the one-block-at-a-time raw-text editing model and the
same operation vocabulary, but replace "regenerate the whole file" by span-preserving serialization, and make writes
atomic with 3-way merge instead of abort-and-ask.

---

## 1. Architecture of an edit

```
keydown / input event (components/editor.cljs box, ui/ls-textarea)
   │
   ├─ text-only change → state :editor/content (in-memory)           ── edit-box-on-change! editor.cljs:1853
   │      └─ 500 ms debounce + input-idle → save-current-block!        ── editor.cljs:1861-1868, :1322
   │
   └─ structural key (Enter/Tab/Backspace@0/Alt+Shift+↑…) → handler   ── editor.cljs:2449, :2728, :2816, :1731
          └─ save-current-block! first, then
             outliner-tx/transact! {:outliner-op …}                    ── modules/outliner/transaction.cljc:9
                 (outliner-core/insert-blocks! | move-blocks! | …)    ── modules/outliner/core.cljs:866-897
                 → ds/transact! (datoms + uuid/ref fixups)             ── modules/outliner/datascript.cljc:122
                     → after-transact-pipelines                        ── datascript.cljc:29-38
                         ├─ pipelines/invoke-hooks                     ── modules/outliner/pipeline.cljs:85
                         │     ├─ recompute :block/path-refs
                         │     ├─ react/refresh! (re-render queries)
                         │     └─ file/sync-to-file per touched page  ── outliner/file.cljs:86
                         │           → file-write-chan → 1 s ratelimit ── outliner/file.cljs:101
                         │           → do-write-file! → save-tree!     ── outliner/file.cljs:46, file/core.cljs:168
                         │           → tree->file-content (whole page) ── file/core.cljs:108
                         │           → fs/write-plain-text-file!       ── fs/node.cljs:21 (compare-with-disk)
                         ├─ undo-redo/listen-db-changes!               ── editor/undo_redo.cljs:247
                         └─ search/sync-search-indice!
```

### 1.1 The transaction macro

`outliner-tx/transact!` (`src/main/frontend/modules/outliner/transaction.cljc:9-56`) binds a transient vector
`*transaction-data*`; each primitive (`op-transact!`, `src/main/frontend/modules/outliner/core.cljs:866`) appends its
`{:tx-data :tx-meta}`. Nested `transact!` calls are flattened into the outermost one (`:nested-transaction?`), so a
user action such as "Enter" = *save current block + insert new block* becomes **one** DataScript transaction, hence one
undo step and one file write. The editor cursor *before* the transaction is captured
(`transaction.cljc:32`) and stored in `[:history/tx->editor-cursor tx-id]` (`datascript.cljc:157`) for undo.

`ds/transact!` (`src/main/frontend/modules/outliner/datascript.cljc:122-178`) also:

- strips non-persisted attrs (`:block/children`, `:block/level`, `:block.temp/*`) (`:125-133`);
- when a block's uuid is swapped during a merge (Backspace/Delete onto a referenced block), rewrites references
  `((from))` → `((to))` (`update-block-refs`, `:59-85`);
- on `:delete-blocks`, **replaces every `((uuid))` and `{{embed ((uuid))}}` pointing to a deleted block by the deleted
  block's text** (`replace-ref-with-content`, `:88-119`) and remembers a revert tx so a subsequent paste of the cut
  blocks restores refs (`src/main/frontend/handler/paste.cljs:109`);
- asserts that no two blocks share the same `(left, parent)` pair (`:164-174`) — the linked-list invariant.

### 1.2 Tree model

`Block` record + `INode` protocol (`src/main/frontend/modules/outliner/tree.cljs:8-22`,
`src/main/frontend/modules/outliner/core.cljs:24-237`). Position is `:block/parent` + `:block/left` (left sibling, or
the parent itself for a first child). `-get-right` / `-get-down` are queries for "who has me as left".
Every insert/move/delete therefore also patches the `:block/left` of the old and new right neighbours
(`build-move-blocks-next-tx` `core.cljs:595`, `fix-non-consecutive-blocks` `:625`, `delete-block` `:643`).
`tree/blocks->vec-tree` (`tree.cljs:55`) rebuilds the nested tree for rendering and serialization.

---

## 2. Editing model (one block, raw text)

| Aspect | Behaviour | Reference |
|---|---|---|
| Enter edit mode | Mouse-down on block content (unless target is a link/checkbox/etc. — `target-forbidden-edit?`) computes the caret from the DOM selection and calls `state/set-editing!` after 5 ms; it first saves any block currently being edited. | `src/main/frontend/components/block.cljs:2154-2221`, `:2136` |
| Programmatic | `edit-block! block pos id` — `pos` may be an int, `:max`, or `[direction line-pos]` (to keep the column when crossing blocks with ↑/↓). Retries 3× if the DOM node is not rendered yet. | `src/main/frontend/handler/editor/property.cljs:38-74`, `:26-36` |
| Text shown | `:block/content` minus hidden built-in properties and minus `:LOGBOOK:` drawer. User properties (`foo:: bar`) **stay visible** as lines in the textarea. | `editor/property.cljs:67-69`, `src/main/frontend/util/property.cljs:18-24`, `:361` |
| Hidden built-ins | `id custom-id background-color heading collapsed created-at updated-at last-modified-at query-table query-properties query-sort-by query-sort-desc ls-type hl-* logseq.macro-* logseq.order-list-type logseq.tldraw.* todo doing now later done` + config `:block-hidden-properties` | `deps/graph-parser/src/logseq/graph_parser/property.cljs:68-79` |
| Max length | Blocks longer than `:block/content-max-length` (default **10 000** chars) cannot enter edit mode; they get selected instead. | `src/main/frontend/state.cljs:680-682`, `:1885-1895` |
| Editor widget | `ui/ls-textarea` (auto-growing textarea) + a hidden `mock-textarea` used to compute caret row/column for popups; `modals` renders whichever autocomplete popup matches `:editor/action`. | `src/main/frontend/components/editor.cljs:643-673`, `:551`, `:596` |
| Heading class | The textarea gets a heading style when content starts with `#`/`heading::`. | `components/editor.cljs:509` |
| Auto-save | Every `on-change` stores content in state, then a **500 ms** timer saves if input has been idle ≥ 500 ms (`:editor-op :auto-save`); the page-properties block is skipped while the caret is inside properties. | `src/main/frontend/handler/editor.cljs:1852-1868` |
| Save on leave | `will-unmount` of the editor saves unless the last op was insert/indent/undo/redo/delete/paste (those already saved). Esc / outside click / `visibilitychange` call `on-hide` → save → `escape-editing` (selects the block). | `src/main/frontend/handler/editor/lifecycle.cljs:35-44`, `src/main/frontend/handler/editor/keyboards.cljs:9-40`, `components/block.cljs:2432-2437`, `editor.cljs:3742` |
| Save gate | `save-current-block!` compares trimmed DB content vs textarea value and saves only if different; skipped during IME composition. `save-block-if-changed!` also rejects an `id::` that collides with another block. | `editor.cljs:1322-1360`, `:392-419` |
| Save pipeline | `wrap-parse-block`: re-inject hidden built-ins (`with-built-in-properties`) and logbook, apply time-tracking (`NOW/DOING` → clock-in/out in LOGBOOK), prefix with `- ` and re-parse through mldoc to compute title/body/refs/marker/priority/properties, strip the `- ` again. | `editor.cljs:298-353`, `util/property.cljs:180-219`, `editor.cljs:256-296` |
| Page properties | First block whose first line contains `:: ` is a **pre-block** (page properties). Saving it updates page `:block/alias`, `:block/tags`, `:block/properties`; a changed `title::` triggers a page rename event. | `modules/outliner/core.cljs:163-182`, `editor.cljs:379-389` |

### 2.1 Raw text ↔ entity

- When parsing a file, a block's `:block/content` is the exact source slice `[start_pos, end_pos)` with the bullet
  (`- `) and the continuation-line indentation (`(level-1) tabs + 2 spaces`) removed
  (`deps/graph-parser/src/logseq/graph_parser/block.cljs:406-421`, `deps/graph-parser/src/logseq/graph_parser/mldoc.cljc:77-98`).
  Properties remain inside the content text. See [[02-markdown-block-syntax]] and [[03-parsing-indexing-search]].
- When serializing (`transform-content`, `src/main/frontend/modules/file/core.cljs:34-90`):
  - `collapsed:: true` is inserted / removed according to `:block/collapsed?` (`:20-32`);
  - `id:: <uuid>` is inserted if the block is referenced (`:block/_refs`) but its content lacks its uuid (`:36-37`, `:87-89`);
  - pre-block (page properties) is emitted without bullet (`:41-48`);
  - a **first top-level Markdown block with `heading` property** is emitted without `- ` (`:53-55`, `:63-64`);
  - every other block: `indent * (level-1)` + `-` + `" "` + trimmed content, continuation lines re-indented with
    `indent + "  "` where `indent` = config `:export/bullet-indentation` (default **tab**,
    `src/main/frontend/state.cljs:553-563`);
  - blocks are joined with a single `\n` (`:108-110`). Blank lines between blocks, `*`/`+` bullets, odd indentation and
    trailing spaces are therefore **normalized away on the first write** of a page.

### 2.2 Cursor handling

- Cursor range on click: `util/caret-range` over rendered text (`components/block.cljs:2200-2203`); position is a
  *text prefix* that is mapped back into the raw string (`state.cljs:1885-1920`).
- Crossing boundaries: ↑ on the first row / ↓ on the last row saves and moves to previous/next *visible* (non-collapsed)
  block, preserving column (`editor.cljs:2558-2605`); ← at position 0 / → at end moves to the end/start of the
  neighbour (`:2607-2653`).
- Undo/redo restores the editor cursor captured before the transaction (`src/main/frontend/handler/history.cljs:10-19`,
  `undo_redo.cljs:167-187`).

---

## 3. Outliner operations catalog

| User action | Default key | Handler | Outliner primitive(s) / notes |
|---|---|---|---|
| New block (split at cursor) | `Enter` | `keydown-new-block` `editor.cljs:2449-2514` → `insert-new-block!` `:536` | Text before cursor stays (`compute-fst-snd-block-text` `:421`), text after cursor (left-trimmed) goes to a **new block with a fresh uuid**. If the current block has children (and is expanded) the new block becomes its **first child**, otherwise next sibling (`outliner-insert-block!` `:429-455`). If cursor is at 0 with text after it, a new empty block is inserted **before** (`insert-new-block-before-block-aux!` `:464`). `:outliner-op :insert-blocks`. |
| Enter special cases | `Enter` | `keydown-new-block` | Inside markup → jump past closing marker; inside code fence/admonition → newline; on `[[page]]` → open/create page; on `((ref))` → open in sidebar; inside a list item → continue list (`dwim-in-list` `:2348`); inside properties → new property line (`dwim-in-properties` `:2288`); **empty last child → outdent** (`outdent-on-enter` `:2240`). |
| Soft newline | `Shift+Enter` | `keydown-new-line-handler` `:2528` → `insert "\n"` `:2258` | Inverted when *document mode* (`t d`) is on. |
| Backspace at pos 0 | `Backspace` | `keydown-backspace-handler` `:2728-2814` → `delete-block!` `:825-869` | Merges current text onto the end of the previous visible block (`move-to-prev-block` `:790`) and deletes the current block; **refused if both the block and its left sibling have children**; refused for the first block of a page unless empty. If the deleted block is referenced, the *previous* block takes over the deleted block's uuid and properties so refs keep working (`:851-856`, uuid swap fixed up in `datascript.cljc:59`). |
| Delete at end | `Delete` | `keydown-delete-handler` `:2704` → `delete-concat` `:2659-2702` | Pulls the next block (first child or right sibling) into the current one; refused if that block has children. Same uuid-adoption trick for referenced blocks. |
| Autopair deletion | `Backspace` | `:2779-2797` | Deleting an opening char deletes its pair (`delete-map` `:1586`). |
| Indent / outdent | `Tab` / `Shift+Tab` | `keydown-tab-handler` `:2834` → `indent-outdent` `:2816`; selection: `on-tab` `:1765` | `indent-outdent-blocks` `core.cljs:802-854`: indent = move as last child of left sibling (expands it if collapsed); outdent = move as right sibling of parent and, by default (**direct outdenting**), the former right siblings become children of the outdented block; `:editor/logical-outdenting? true` keeps them in place (`core.cljs:841`). Non-consecutive selections are rejected. |
| Move up / down | `Alt+Shift+↑/↓` (mac `⌘⇧↑/↓`) | `move-up-down` `editor.cljs:1731` | `move-blocks-up-down` `core.cljs:760-800`: swaps with sibling; at boundary moves into the previous parent's last child / next parent's first child position. |
| Collapse / expand | `Mod+↑` / `Mod+↓`, `Mod+;` toggle, `t o` toggle all | `collapse!` `:3547`, `expand!` `:3510`, `toggle-collapse!` `:3586`, `set-blocks-collapsed!` `:3482` | `save-block` with `:block/collapsed?`; persisted as `collapsed:: true` in the file (`file/core.cljs:20-32`). Without a target, collapses/expands **one level** of the whole page. In reference panels collapse is UI-only (`skip-collapsing-in-db?` `:3477`). |
| Select blocks | `Esc` (select current), `Shift+↑/↓`, `Alt+↑/↓`, `Mod+Shift+A` all, `Mod+A` parent, Shift-click range, Mod-click toggle | `escape-editing` `:3742`, `select-up-down` `:2545`, `select-parent` `:3696`, `components/block.cljs:2165-2193` | Selection is DOM-based (`state/get-selection-blocks`). |
| Multi-block ops | `Backspace/Delete`, `Mod+C`, `Mod+X`, `Tab`, `Mod+Enter`, `Alt+Shift+↑/↓` | `shortcut-delete-selection` `:3140`, `copy-selection-blocks` `:1001`, `cut-selection-blocks` `:1079`, `on-tab` `:1765`, `cycle-todos!` `:745` | Copy puts Markdown (export with tab indentation, `id::` stripped), HTML, and an internal EDN payload (`web application/logseq`) on the clipboard (`paste.cljs:87-99`). Cut = copy + `delete-blocks`. |
| Drag & drop | mouse on bullet | `block-drag-over` `components/block.cljs:2623` → `block-drop` `:2659` → `dnd/move-blocks` `src/main/frontend/handler/dnd.cljs:11-58` | Drop zone: within 16 px of top → **before** (`:top`), x-offset > 50 px → **child** (`:nested`), else **sibling after**. `Alt` while dropping a single block inserts a `((ref))` instead of moving (and persists `id::`). Different page formats refused. External text drop → new block; files → asset upload. |
| Zoom in / out | `Mod+.` / `Mod+,` (Win/Linux `Alt+→/←`), click bullet | `zoom-in!` `:1188`, `zoom-out!` `:1200` | Routes to `/page/<block-uuid>`; block becomes the root of the view; breadcrumb shows ancestors (`components/block.cljs:2564`). |
| Copy block ref | `Mod+C` while editing with no selection; context menu | `shortcut-copy` `:3156` → `copy-block-ref!` `:968` | `set-blocks-id!` (`:955`) writes `id:: <uuid>` property to the block (`batch-set-block-property!` `editor/property.cljs:76`) then copies `((uuid))`. |
| Copy block embed | `Mod+E` | `copy-current-block-embed` `:3153` | Same, copies `{{embed ((uuid))}}`. Multi-selection: `copy-block-refs` `:1016`, `copy-block-embeds` `:1053`. |
| Replace ref with text / embed | `Mod+Shift+R`, context menu | `replace-block-reference-with-content-at-point` `:3752`, `replace-ref-with-text!` `:3782`, `replace-ref-with-embed!` `:3797` | Text-only edits inside the textarea. |
| Paste | `Mod+V` | `editor-on-paste!` `src/main/frontend/handler/paste.cljs:230` | Internal copied blocks → `paste-blocks` (`editor.cljs:2013`) keeping uuids only on *cut*. External text: HTML → Markdown via `html-parser`; if text looks like Markdown list/headings (`^\s*([-+*]\|#+)\s+`, `paste.cljs:101`) it is parsed with mldoc into blocks (`paste-text-parseable` `:23`); if it has blank-line separated paragraphs each paragraph becomes a block (`:34`); otherwise inserted inline. URLs to YouTube/Twitter are wrapped in macros (`:49`). Empty target block is replaced by the first pasted block (`replace-empty-target?`). |
| Raw paste | `Mod+Shift+V` | `editor-on-paste-raw!` `paste.cljs:262` | Inserts clipboard text verbatim into the textarea. |
| Cycle TODO | `Mod+Enter` | `cycle-todo!` `editor.cljs:757` | Text edit of the marker: `TODO→DOING→DONE→(none)→TODO` or `LATER→NOW→DONE→(none)→LATER` depending on `:preferred-workflow` (`src/main/frontend/util/marker.cljs:40-73`). Checkbox click: `check`/`uncheck` (`editor.cljs:708-730`); repeated tasks advance their SCHEDULED/DEADLINE (`update-timestamps-content!` `:676`). Time-tracking clocks into `:LOGBOOK:` (`:256-296`). |
| Set heading | `/h1`…`/h6`, context menu "Heading" (auto/1-6) | `set-heading!` `:3864`, `set-heading-aux!` `:3822` | Markdown: numeric heading = literal `#`-prefix in content (`commands/set-markdown-heading` `commands.cljs:624`); "auto heading" = `heading:: true` property. Org: `heading` property. |
| Templates | `/Template`, default journal template | `insert-template!` `editor.cljs:2125-2189` | Copies the `template::` block (and children; `template-including-parent:: false` drops the root) with **new uuids**, strips `template*`/`id` props, resolves dynamic variables (`<% today %>`, …) and inserts after/under target. |
| Properties | typing `::` | `handle-last-input` `:1875`; `property-on-chosen-handler` `:2211` | Pure text edits; `batch-set-block-property!` (`editor/property.cljs:76-122`) rewrites the property lines of a block's content (order preserved via `:block/properties-order`). |
| Number list | `t n`, `/Number list`, typing `1. ` | `toggle-blocks-as-own-order-list!` `editor.cljs:96`, `:1888-1891` | Stored as `logseq.order-list-type:: number` hidden property. |
| Page rename | edit page title / `title::` | `page-handler/rename!` `src/main/frontend/handler/page.cljs:618-645` | Same lower-cased name → `rename-page-aux` (`:450`): update page entity, `title::` in pre-block if needed, **rename file** (`rename-file!` `:201`; journals keep file name), then `rename-update-refs!` (`:393`) rewrites `[[old]]`, `#old`/`#[[old]]`, `old::` property keys and property values in **every referencing block** (regex based: `:229-311`) and calls `sync-to-file` on each referencing page → every referencing file is rewritten. Namespaced children (`old/x`) and pages whose name contains `[[old]]` are renamed recursively (`:516-566`). |
| Page merge | rename to an existing page name | `merge-pages!` `page.cljs:568-616` | Moves all blocks of *from* under *to* (appended after its last top-level block), rewrites refs as above, then deletes *from* (and its file). |
| Delete page | page menu | `page.cljs:352-383` | Retracts blocks, page entity (kept if it is an alias target) and unlinks the file. |
| Delete block with references | any delete | `datascript.cljc:88-119` | References/embeds to the deleted block are **inlined** with the deleted text (properties removed) in the referencing blocks. |

### 3.1 Primitive semantics worth copying

- `insert-blocks` (`core.cljs:520-593`): input is a flat, ordered list of blocks with `:block/level`;
  `sibling?` vs child; `keep-uuid?` (cut/DnD) vs new uuids (copy/template); `replace-empty-target?` (pasting onto an
  empty block replaces it, adopting its uuid); `:paste` fixes broken top-level `left` links
  (`fix-top-level-blocks` `:437`). Moves reuse `insert-blocks` with `keep-uuid?` and keep `:db/id`.
- `move-blocks` (`core.cljs:717-758`): refuses to move a block into its own descendant and no-ops when moving to the
  original position; moving across pages rewrites `:block/page` of all descendants (`:741-744`).
- `delete-blocks` (`core.cljs:654-707`): with `children? false` (Backspace merge) the children of the deleted
  block are re-parented to its left sibling (`:211-224`).

---

## 4. Autocomplete & commands

Triggers are detected after each keystroke in `handle-last-input` (`editor.cljs:1875-1936`) and via autopair
(`autopair` `:1591-1616`, `autopair-map` `:1565-1576`: `[] {} () `` ~~ ** __ ^^ == // ++`). The active popup is
`:editor/action`; Esc closes the popup before leaving edit mode (`keyboards.cljs:15-23`).

| Trigger | Action | Popup / data source | Insert result |
|---|---|---|---|
| `[[` (autopair of `[`) | `:page-search` | `components/editor.cljs:105-169`, `get-matched-pages` (`editor.cljs:1642`) — fuzzy page search excluding the current page; non-existing query offered as "New page" | `[[Page]]` via `page-handler/on-chosen-handler` (`src/main/frontend/handler/page.cljs:780`) |
| `#` at line start or after whitespace | `:page-search-hashtag` | same popup | `#tag` or `#[[multi word]]` (`page.cljs:792-812`) |
| `((` | `:block-search` | `components/editor.cljs:178-226`, `get-matched-blocks` (`editor.cljs:1655`) — full-text block search (limit 20) excluding current block and its parents | `((uuid))` and **writes `id::` to the chosen block** (`block-on-chosen-handler` `editor.cljs:1944-1966`) |
| `/` at line start or after whitespace | `:commands` | `components/editor.cljs:34`; list `commands-map` (`src/main/frontend/commands.cljs:218-313`) | Page/Block reference & embed, Link, Image link, Underline, Template, Upload asset, h1–h6, Tomorrow/Yesterday/Today/Current time/Date picker, Number list/children, TODO/DOING/LATER/NOW/DONE/WAITING/CANCELED, Deadline/Scheduled, Priority A/B/C, Query, Zotero, Query function, Calculator, Draw, Embed HTML/Video/YouTube timestamp/Twitter, Code block, + user `:commands` + plugins |
| `<` | `:block-commands` | `components/editor.cljs:89`; `block-commands-map` (`commands.cljs:189-216`) | `#+BEGIN_QUOTE … #+END_QUOTE` style blocks: Quote, Src (```` ``` ````), Query, Note/Tip/Important/Caution/Pinned/Warning, Example, Export, Verse, Ascii, Center, Comment |
| `::` at line start | `:property-search` | `components/editor.cljs:250`, `get-matched-properties` (`editor.cljs:1674`) | `key:: ` then `:property-value-search` (`components/editor.cljs:269`) with existing values |
| `/Template` | `:template-search` | `components/editor.cljs:227` | `insert-template!` |
| `/Date picker`, `/Deadline`, `/Scheduled` | `:datepicker` | `components/editor.cljs:623` | `[[Journal date]]` or `SCHEDULED: <2024-01-01 Mon>` |
| Popup keys | `Enter` complete, `Shift+Enter` shift-complete, `↑/↓` or `Ctrl+P/N`, `Mod+O` open | `src/main/frontend/modules/shortcut/config.cljs:165-178` | |

Commands are expressed as step vectors (`[:editor/input "..." {:backward-pos n}] [:editor/search-page]`) executed by
`commands/handle-steps` (`commands.cljs:724`) — a nice declarative design to imitate.

---

## 5. Write path (DB → file)

| Step | Detail | Reference |
|---|---|---|
| 1. Hook | For each page touched by the tx (excluding `:from-disk?`, `:new-graph?`, `:replace?` txs) call `sync-to-file`. | `src/main/frontend/modules/outliner/pipeline.cljs:85-120`, `:12-15` |
| 2. Queue | `[repo page-id outliner-op timestamp]` put on `file-write-chan` (immediate write while importing). | `src/main/frontend/modules/outliner/file.cljs:86-97` |
| 3. Batch | `<ratelimit` flushes the channel every **1000 ms**, deduplicating per page. Started at app init. | `outliner/file.cljs:17`, `:101-117`, `src/main/frontend/util.cljc:1154-1200`, `src/main/frontend/handler.cljs:240` |
| 4. Large pages | Pages with **> 500 blocks** are re-queued until the user has been idle **3 s**. | `outliner/file.cljs:55-59`, `state.cljs:1709-1719` |
| 5. Serialize | Pull **all blocks of the page**, rebuild the tree, `tree->file-content` → full text. Empty result (not caused by deletion) is refused as a bug. Page without a file gets a path: journals → `journals/yyyy_MM_dd.md`, others → `pages/<sanitized>.md`. | `outliner/file.cljs:46-70`, `src/main/frontend/modules/file/core.cljs:115-165` |
| 6. Write | `alter-files-handler!` → `fs/write-plain-text-file!` with `:old-content` = DB's known file content. | `src/main/frontend/handler/file.cljs:203-234`, `src/main/frontend/fs.cljs:92-115` |
| 7. Conflict check (Electron) | `stat`; read disk; compare `trim(disk) == trim(old-content)`. Mismatch (and file exists, not `edn/css/excalidraw`, not in `.recycle`) → **no write**, emit `:file/not-matched-from-disk`. | `src/main/frontend/fs/node.cljs:16-50` |
| 8. Dialog | Clears editing and opens modal "File X has been modified on the disk." with a diff and two editable panes (disk vs DB) letting the user pick/merge. | `src/main/frontend/handler/events.cljs:374-380`, `src/main/frontend/components/diff.cljs:27-90` |
| 9. Actual write | IPC `writeFile` → `fs.writeFileSync` (chmod 644 if read-only) — **not atomic**; on failure the content is saved to `logseq/bak/…` and a notification shown. Afterwards store `mtime` and new content in DB. | `src/electron/electron/handler.cljs:117-141`, `fs/node.cljs:53-62` |
| 10. Backups | `backupDbFile` keeps the previous version in `logseq/bak/<path>/<ISO-time>.Desktop.md` when the old text had deletions; keeps the latest **6** versions. | `src/electron/electron/handler.cljs:70-80`, `src/electron/electron/backup_file.cljs:27-51` |
| 11. Own-write echo | The file watcher receives the change; since trimmed content equals the DB copy nothing happens. | `src/main/frontend/fs/watcher_handler.cljs:91-94` |

### 5.1 External modifications (watcher)

`handle-changed!` (`src/main/frontend/fs/watcher_handler.cljs:58-137`): on `add`/`change` with trimmed content ≠ DB
content, back up the DB version (`backup-file!`) and **re-parse the whole file** into the DB
(`alter-file … :from-disk? true`, `:44-56`), then add missing `id::` properties for blocks referenced from the new
content (`set-missing-block-ids!` `:28-42`). Journal files equal to the default template or to `-` are ignored
(`:95-101`). `unlink` deletes the page. The block currently being edited is not protected: a re-parse while editing
can lose the in-progress text (the conflict dialog only exists on the *write* side).

### 5.2 How much is rewritten

Always the **whole page file** (`tree->file-content` of every block). Consequences:
- first edit of a hand-written file reformats it (indentation to tabs, `* `→`- `, blank lines removed, `collapsed::`
  normalized, trimming);
- concurrent edits (git, Syncthing, another editor) produce whole-file conflicts and the abort-and-ask dialog;
- rename of a popular page rewrites N files.

---

## 6. Undo / redo

| Aspect | Behaviour | Reference |
|---|---|---|
| Stack unit | Each outliner DataScript tx produces `{:tx-id :blocks :txs (datoms) :tx-meta :pagination-blocks-range :app-state (route, sidebar)}` | `src/main/frontend/modules/editor/undo_redo.cljs:247-272` |
| Grouping | One `outliner-tx/transact!` = one entry. Follow-up `:replace?` txs (path-refs recompute) are **appended to the previous entry**. Txs touching only timestamps are ignored. | `undo_redo.cljs:253-260` |
| Typing granularity | No explicit coalescing: each 500 ms auto-save is a `:save-block` entry. Undo while editing first saves the current block. | `editor.cljs:1861`, `src/main/frontend/handler/history.cljs:40-51` |
| Inversion | Datoms are reversed and `add`/`retract` flipped. | `undo_redo.cljs:113-123`, `:189-211` |
| Redo | Cleared by any new tx. | `undo_redo.cljs:256` |
| Cursor | Restores the editor cursor/block saved for that tx (or the previous tx for `save-block`). | `undo_redo.cljs:167-201`, `history.cljs:10-19` |
| Scope | Per graph, in-memory only; optional "page-only" mode undoes only entries touching the current page. | `undo_redo.cljs:16-28`, `:132-156`, `:232-237` |
| Files | Undo is just another tx → the page file is re-serialized and written again. | — |

---

## 7. Keyboard shortcuts (editor-relevant, ~60)

From `src/main/frontend/modules/shortcut/config.cljs` (line numbers point at the binding). `mod` = Cmd on macOS,
Ctrl elsewhere.

| Id | Binding (Win/Linux · mac) | Line |
|---|---|---|
| `:editor/new-block` | `Enter` | 205 |
| `:editor/new-line` | `Shift+Enter` | 208 |
| `:editor/backspace` / `:editor/delete` | `Backspace` / `Delete` | 199, 202 |
| `:editor/indent` / `:editor/outdent` | `Tab` / `Shift+Tab` | 319, 322 |
| `:editor/move-block-up` / `-down` | `Alt+Shift+↑/↓` · `Mod+Shift+↑/↓` | 285, 288 |
| `:editor/cycle-todo` | `Mod+Enter` | 270 |
| `:editor/escape-editing` | `Esc` (handled by editor) | 195 |
| `:editor/open-edit` | `Enter` (on selected block) | 292 |
| `:editor/up` / `:editor/down` | `↑`/`Ctrl+P`, `↓`/`Ctrl+N` | 273, 276 |
| `:editor/left` / `:editor/right` | `←` / `→` | 279, 282 |
| `:editor/select-block-up` / `-down` | `Alt+↑` / `Alt+↓` | 295, 298 |
| `:editor/select-up` / `-down` | `Shift+↑` / `Shift+↓` | 301, 304 |
| `:editor/select-all-blocks` | `Mod+Shift+A` | 343 |
| `:editor/select-parent` | `Mod+A` (expands selection to parent) | 346 |
| `:editor/delete-selection` | `Backspace`/`Delete` (selection) | 307 |
| `:editor/copy` / `:editor/copy-text` / `:editor/cut` | `Mod+C` / `Mod+Shift+C` / `Mod+X` | 325, 328, 331 |
| `:editor/copy-embed` | `Mod+E` | 261 |
| `:editor/paste-text-in-one-block-at-point` | `Mod+Shift+V` | 264 |
| `:editor/undo` / `:editor/redo` | `Mod+Z` / `Mod+Shift+Z`, `Mod+Y` | 334, 337 |
| `:editor/expand-block-children` / `collapse-` | `Mod+↓` / `Mod+↑` | 310, 313 |
| `:editor/toggle-block-children` | `Mod+;` | 316 |
| `:editor/toggle-open-blocks` | `t o` | 533 |
| `:editor/zoom-in` / `:editor/zoom-out` | `Alt+→`/`Alt+←` · `Mod+.`/`Mod+,` | 349, 352 |
| `:editor/follow-link` / `:editor/open-link-in-sidebar` | `Mod+O` / `Mod+Shift+O` | 214, 217 |
| `:editor/bold` `:italics` `:highlight` `:strike-through` | `Mod+B` `Mod+I` `Mod+Shift+H` `Mod+Shift+S` | 220–229 |
| `:editor/insert-link` | `Mod+L` | 340 |
| `:editor/clear-block` | `Alt+L` · `Ctrl+L` | 232 |
| `:editor/kill-line-before` / `-after` | `Alt+U` · `Ctrl+U` / `Alt+K` | 235, 238 |
| `:editor/beginning-of-block` / `end-of-block` | `Alt+A` / `Alt+E` (non-mac) | 241, 244 |
| `:editor/forward-word` / `backward-word` | `Alt+F`/`Alt+B` · `Ctrl+Shift+F/B` | 247, 250 |
| `:editor/forward-kill-word` / `backward-kill-word` | `Alt+D` / `Alt+W` · `Ctrl+W` | 253, 256 |
| `:editor/replace-block-reference-at-point` | `Mod+Shift+R` | 259 |
| `:editor/insert-youtube-timestamp` | `Mod+Shift+Y` | 267 |
| `:editor/toggle-number-list` | `t n` | 358 |
| `:editor/toggle-undo-redo-mode` | (unbound) | 355 |
| `:ui/toggle-brackets` | `Mod+C Mod+B` | 361 |
| `:ui/toggle-document-mode` | `t d` | 472 |
| `:auto-complete/complete` `prev` `next` `shift-complete` `open-link` | `Enter`, `↑/Ctrl+P`, `↓/Ctrl+N`, `Shift+Enter`, `Mod+O` | 165–178 |
| `:date-picker/*` | `Enter`, `←/→` day, `↑/↓` week | 54–68 |
| `:go/search` / `:go/search-in-page` | `Mod+K` / `Mod+Shift+K` | 364, 368 |
| `:command-palette/toggle` | `Mod+Shift+P` | 366 |
| `:go/journals` `:go/home` `:go/all-pages` `:go/graph-view` | `g j`, `g h`, `g a`, `g g` | 384, 442, 445, 448 |
| `:go/next-journal` / `:go/prev-journal` / `:go/tomorrow` | `g n` / `g p` / `g t` | 463, 466, 460 |
| `:go/backward` / `:go/forward` | `Mod+[` / `Mod+]` | 387, 390 |
| `:sidebar/open-today-page` | `Alt+Shift+J` · `Mod+Shift+J` | 396 |
| `:sidebar/close-top` / `:sidebar/clear` | `c t` / `Mod+C Mod+C` | 399, 402 |
| `:ui/toggle-right-sidebar` / `left-sidebar` | `t r` / `t l` | 478, 481 |
| `:ui/toggle-wide-mode` | `t w` | 516 |
| `:command/toggle-favorite` | `Mod+Shift+F` | 493 |
| `:ui/toggle-help` | `Shift+/` | 484 |

Shortcut handler groups (`:shortcut.handler/block-editing-only`, `…/editor-global`, …) decide which bindings are
active in edit mode vs selection mode (`components/editor.cljs:651`).

---

## 8. Rendering (non-edit mode)

| Element | Component | Notes |
|---|---|---|
| Block container | `block-container` / `block-container-inner` `src/main/frontend/components/block.cljs:2940`, `:2817` | Bullet + collapse arrow (`block-control` `:1743`), content, children (`block-children` `:1706`), drop separators. |
| Content | `block-content` `:2292`, `build-block-title` `:1930`, `inline` `:1564` | Renders the mldoc AST: marker (`marker-switch` `:1861`, `marker-cp` `:1879`), checkbox (`block-checkbox` `:1829`), priority (`priority-cp` `:1904`), title inlines, body (`markup-elements-cp` `:3358`), SCHEDULED/DEADLINE (`timestamp-cp` `:2104`), properties table (`properties-cp` `:2070`, hidden props filtered), logbook/clock summary (`:3119`, `:2242`). |
| Page ref `[[x]]` / `#x` | `page-reference` `:756`, `page-cp` `:677` | Hover preview (`page-preview-trigger` `:606`); brackets visibility toggle `Mod+C Mod+B`. |
| Block ref `((uuid))` | `block-reference` `:866` | Inline title of referenced block; click opens/zooms; shift-click to sidebar. |
| Embeds | `block-embed` `:798`, `page-embed` `:816`, `macro-embed-cp` `:1265` | Embedded blocks are fully editable in place (nested editor). |
| Macros & queries | `macro-cp` `:1464`, `macro-query-cp` `:1249`, video/youtube/twitter `:1290-1378` | `{{query …}}` and advanced queries render result lists/tables (`frontend.components.query`). |
| Refs count | `block-refs-count` `:2378` | Number bubble to expand linked refs of the block inline (`block-content-or-editor` `:2475-2482`). |
| Lazy rendering | `lazy-blocks` `:3398`, `blocks-container` `:3434` | Not virtualized: an **infinite list** that renders the first `initial-blocks-length` = **50** blocks and loads `step-loading-blocks` = **25** more on scroll (`src/main/frontend/db/model.cljs:28-30`). Journals that are not "today" render their blocks via `ui/lazy-visible` (only when scrolled into view) (`src/main/frontend/components/journal.cljs:57-62`). |
| Breadcrumb | `breadcrumb` `:2564` | For zoomed blocks and reference results. |

## 9. Journals, sidebar, references

- **Journals page** (`src/main/frontend/components/journal.cljs:75-93`): `infinite-list` of the latest journals;
  `load-more-journals!` adds **7** at a time (`src/main/frontend/handler/page.cljs:676-679`); reaching the top calls
  `create-today-journal!`.
- **Today's journal** (`page.cljs:823-852`) is checked at startup and **every 5 s** (`src/main/frontend/handler.cljs:60-72`).
  If the page is empty and the file does not exist (or is blank), the page is created (DB only — "Don't create the
  journal file until user writes something", `handler.cljs:69`) and `:journal/insert-template` inserts
  `:default-templates {:journals "…"}` (`src/main/frontend/handler/events.cljs:723-731`). Writes coming from a
  journal template are skipped by the file hook (`pipeline.cljs:12-15`), so a pristine journal never touches disk.
- **Empty page**: a dummy "Click here to edit…" block creates the first real block on click (`src/main/frontend/components/page.cljs:113-150`).
- **Right sidebar** (`src/main/frontend/components/right_sidebar.cljs`, `state/sidebar-add-block!` `src/main/frontend/state.cljs:1121`):
  stack of pages/blocks/contents/graph/help items, each a full editable outline (Shift-click any ref or bullet to open).
- **Linked references** (`src/main/frontend/components/reference.cljs:129-254`, query `get-page-referenced-blocks`
  `src/main/frontend/db/model.cljs:1255`): blocks whose `:block/path-refs` contain the page (or aliases), grouped by
  page with breadcrumbs, filterable by include/exclude page filters stored in page properties `filters::`
  (`page-handler/save-filter!`). Rendered collapsed beyond `:ref/default-open-blocks-level`
  (`editor.cljs:3808-3820`).
- **Unlinked references** (`reference.cljs:256-292`, `model.cljs:1325-1350`): lazy (on expand) full scan of block
  contents matching the page title/aliases as plain text, excluding the page itself. A "link" action is not built in.

---

## Requirements for Bitacora

1. **MUST** edit exactly one block at a time as raw Markdown text (multi-line), rendering all other blocks; entering
   edit mode must place the caret at the clicked text position.
2. **MUST** hide `id::`, `collapsed::`, boolean `heading::`, and the other hidden built-in properties from the edit
   buffer and re-inject them verbatim (same position and order as on disk) on save; user properties stay visible.
3. **MUST** auto-save the edit buffer on a short debounce (≈500 ms idle) and on blur/Esc/navigation, and save
   before any structural operation.
4. **MUST** implement the core operations with Logseq semantics: split on Enter (cursor-based, first child if the
   block has expanded children), Shift+Enter newline, Backspace-at-0 merge into previous visible block,
   Delete-at-end merge of next, Tab/Shift+Tab (direct outdenting by default, logical outdenting as an option),
   Alt+Shift+↑/↓ move, collapse/expand persisted as `collapsed:: true`, Enter on empty last child = outdent.
5. **MUST** preserve block identity through merges: when a referenced block is merged away, the survivor adopts its
   `id::` (Logseq `editor.cljs:851-856`) or references must be rewritten.
6. **MUST** generate and persist `id:: <uuid>` whenever a block reference/embed to a block is created
   (copy ref, `((` autocomplete, Alt-drag).
7. **MUST** group each user action into one transaction = one undo step = one write of each touched file.
8. **MUST** detect external modifications before writing (content hash of last-known disk bytes) and never silently
   overwrite them.
9. **MUST** write atomically (temp file + rename) — Logseq does not, and loses data on crash mid-write.
10. **MUST** support paste of multi-line Markdown as a block tree and internal copy/cut/paste of block subtrees
    (new uuids on copy, kept uuids on cut).
11. **MUST** support `[[`, `#`, `((`, `/` autocomplete and `::` property autocomplete.
12. **MUST** implement page rename that updates `[[old]]`, `#old`, `#[[old]]` and property values in all files, and
    renames the page file (not journals).
13. **SHOULD** serialize without reformatting untouched blocks (byte-preserving) — a deliberate improvement over
    Logseq's whole-file regeneration (see [[block-editor]]).
14. **SHOULD** keep Logseq's write debounce order of magnitude (≤1 s) so that Logseq running on the same graph sees
    consistent files, and ignore watcher echoes of its own writes.
15. **SHOULD** implement the `<` block commands, date picker, templates (with `<% today %>` style variables and
    `template-including-parent`), cycle TODO with both workflows, headings (`#` prefix vs `heading:: true`).
16. **SHOULD** render long pages lazily/virtualized (Logseq: 50 + 25 per step) and journals as an infinite list
    loading 7 at a time, creating today's journal in memory only until the first edit.
17. **SHOULD** support drag & drop with the three drop zones (before / child / after) and Alt-drop = insert ref.
18. **SHOULD** offer the same default keybindings (table §7) so Logseq users keep their muscle memory.
19. **SHOULD** inline the text of a deleted block into blocks that referenced it (or warn), as Logseq does.
20. **SHOULD** back up the previous file version before overwriting when content was removed (Logseq keeps 6 in
    `logseq/bak/`).

## Open questions

- Should Bitacora write the `logseq/bak/` backups in the same location/format for interoperability, or keep its own
  history (e.g. relying on git)?
- Logseq normalizes indentation to tabs on first write. When Bitacora edits a block in a file that a Logseq user will
  later open, does Logseq tolerate mixed styles produced by span-preserving writes? (It parses both; a quick test
  corpus is needed — see [[02-markdown-block-syntax]].)
- Should typing be coalesced into larger undo steps than Logseq's 500 ms auto-save granularity?
- Logseq's Backspace/Delete merges are refused when both blocks have children; do we keep that restriction or
  implement a well-defined merge of children lists?
- Unlinked references "link" action and page-only undo mode: worth implementing?
- How should Bitacora behave while a block is being edited and the same file changes on disk (Logseq has no
  protection on the read side)?
- Should Org-mode files be editable in the MVP or read-only?
