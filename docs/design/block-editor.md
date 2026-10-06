# Bitacora block editor — design

> Status: proposal · Inputs: [[04-editor-outliner-operations]] (how Logseq 0.10.15 edits blocks),
> [[01-file-graph-layout]], [[02-markdown-block-syntax]], [[03-parsing-indexing-search]].
> Logseq references are `path:LINE` relative to the Logseq repo root (`/www/Bitacora/logseq`).

## Summary

Bitacora edits Logseq Markdown graphs as an outliner. It reuses Logseq's interaction model, which users already know:
one block at a time is edited as raw text, everything else is rendered, and structure changes come from keys
(Enter, Tab, Backspace, Alt+Shift+↑/↓). It differs from Logseq in three ways:

1. **The file is the source of truth, not a database.** Each open page is an in-memory tree of blocks. Every block
   remembers the exact byte span it came from. Logseq instead regenerates the whole file from DataScript on every
   change (`src/main/frontend/modules/file/core.cljs:108`).
2. **Serialization preserves bytes.** Clean blocks are written back verbatim. Only blocks whose text or depth changed
   are re-emitted, and they use the file's own indentation style. A one-word edit is a one-line git diff.
3. **Writes are safe.** Bitacora checks the content hash of the file on disk against the last content it read or
   wrote. If they differ, it runs a block-level 3-way merge. Files are written atomically (temp file, fsync, rename).
   Logseq instead compares trimmed text and aborts with a dialog (`src/main/frontend/fs/node.cljs:44-50`), and it
   writes with `writeFileSync` (`src/electron/electron/handler.cljs:125`).

Edits are expressed as a small set of **invertible primitive ops** (`Op`). User commands compile into `Transaction`s,
which drive undo/redo, file writes and index updates.

---

## 1. Goals and non-goals

| Goals | Non-goals (for now) |
|---|---|
| Logseq-compatible Markdown output that Logseq 0.10.x parses identically | Org-mode editing (read-only render in MVP) |
| Minimal, local diffs; untouched bytes never change | Whiteboards, PDF annotations, flashcards |
| One-block raw-text editing with Logseq keybindings | Real-time collaborative editing (CRDT) |
| Crash-safe atomic writes, no silent overwrite of external edits | DB-graph (SQLite) support |
| Smooth editing of 5,000+ block pages (virtualized list) | Plugin API |

---

## 2. In-memory document model

### 2.1 Types

```rust
/// Session-local, never persisted. Stable across edits, moves and reloads of the same page
/// (reload remaps by uuid / text alignment, see §6.3).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct BlockId(u64);

pub struct Graph {
    pub root: PathBuf,
    pub pages: HashMap<PageKey, Page>,     // only loaded (open / recently touched) pages
    pub index: Arc<GraphIndex>,            // refs, uuids, names — see [[03-parsing-indexing-search]]
    pub history: History,                  // §4
    pub writer: WriteQueue,                // §5
}

pub struct Page {
    pub key: PageKey,                      // lower-cased sanitized name
    pub title: String,                     // original name
    pub path: Option<RelPath>,             // None = virtual page (e.g. today's journal before first edit)
    pub format: Format,                    // Markdown | Org (Org read-only in MVP)
    pub disk: DiskSnapshot,                // what we believe is on disk
    pub style: FileStyle,                  // detected conventions, used only for re-emitted lines
    pub preamble: Option<Preamble>,        // front matter or page-properties pre-block
    pub roots: Vec<BlockId>,
    pub blocks: SlotMap<BlockId, Block>,
    pub dirty: bool,
}

pub struct DiskSnapshot {
    pub bytes: Arc<[u8]>,                  // last bytes read from or written to disk (merge base; in memory only, ADR-017)
    pub hash: blake3::Hash,
    pub mtime: SystemTime,
    pub len: u64,
}

pub struct FileStyle {
    pub indent_unit: IndentUnit,           // Tab | Spaces(n), detected (majority); default Tab like Logseq
    pub line_ending: LineEnding,           // Lf | CrLf
    pub bullet: char,                      // '-' default
    pub final_newline: bool,
}

pub struct Block {
    pub id: BlockId,
    pub uuid: Option<Uuid>,                // from `id::` if present; generated lazily when referenced
    pub parent: Option<BlockId>,
    pub children: Vec<BlockId>,
    /// Logical content exactly like Logseq's :block/content: the bullet and the continuation
    /// indentation are stripped, but property lines (including hidden ones such as
    /// `id::` and `collapsed::`) and drawers are kept.
    pub text: String,
    pub origin: Option<Origin>,            // None for blocks created in this session
    pub parsed: OnceCell<BlockAst>,        // lazily parsed inline/props/marker; invalidated on SetText
}

/// Where the block's *own* lines came from (not its children).
pub struct Origin {
    pub span: Range<usize>,                // byte range in disk.bytes: bullet line .. before first child/next block
    pub depth: u16,                        // 0 = top level
    pub text_hash: u64,                    // hash of `text` when loaded → cheap "clean?" test
    pub layout: LineLayout,                // exact prefixes per line, needed to re-indent faithfully
}

pub struct LineLayout {
    pub bullet_prefix: String,             // e.g. "\t\t- " or "    * "
    pub cont_prefix: String,               // e.g. "\t\t  "
    pub trailer: Range<usize>,             // blank lines / trailing whitespace after the block's text
}
```

Collapsed state, `id::`, `heading::` and the other hidden properties live **in `text`**, as on disk. The UI derives
`collapsed` from the parsed properties. As a result, collapsing a block is an ordinary `SetText` that touches one
line of one block. This mirrors how Logseq persists `collapsed:: true` (`src/main/frontend/modules/file/core.cljs:20-32`)
but without rewriting the whole file.

### 2.2 Clean vs dirty blocks

A block is **clean** if `hash(text) == origin.text_hash` and its current depth equals `origin.depth`. Siblings order,
parent identity and page do *not* matter. Each block's span holds only its own lines, so a clean block can be emitted
verbatim anywhere at the same depth. Moving a subtree to the same depth shows up in git as moved lines, not
rewritten lines.

### 2.3 Edit projection (what the textarea shows)

Logseq strips hidden built-in properties and the `:LOGBOOK:` drawer before editing
(`src/main/frontend/handler/editor/property.cljs:67-69`). It re-injects them on save (`src/main/frontend/handler/editor.cljs:298-353`,
`src/main/frontend/util/property.cljs:180-219`). Logseq re-injects them *right after the title line*, which can
reorder lines. Bitacora keeps the exact position instead:

```rust
pub struct EditProjection {
    pub visible: String,                         // shown in the editor
    hidden: Vec<HiddenLine>,                     // removed lines + anchor (index of preceding visible line)
}
impl EditProjection {
    pub fn from_text(text: &str, hidden_keys: &HiddenKeys) -> Self;
    pub fn to_text(&self, edited_visible: &str) -> String; // re-inserts hidden lines at their anchors (clamped)
}
```

Hidden keys: `id custom-id collapsed heading(bool) background-color created-at updated-at last-modified-at
query-table query-properties query-sort-by query-sort-desc logseq.order-list-type ls-type hl-* logseq.macro-*` plus the
`:LOGBOOK:` drawer and config `:block-hidden-properties` (`deps/graph-parser/src/logseq/graph_parser/property.cljs:68-79`).

---

## 3. Operations

### 3.1 Primitive ops (invertible, page-local except page ops)

```rust
pub enum Op {
    /// Insert a detached subtree as child `index` of `parent` (None = page root).
    InsertSubtree { page: PageKey, parent: Option<BlockId>, index: usize, subtree: Subtree },
    /// Remove a block and all its descendants; the removed subtree is captured for the inverse.
    RemoveSubtree { page: PageKey, id: BlockId, captured: Option<Subtree> },
    /// Move a block (with its subtree) within a page or across pages.
    Move { id: BlockId, from: Position, to: Position },
    /// Replace the full logical text of a block (small blocks) …
    SetText { id: BlockId, before: String, after: String },
    /// … or a range edit, for typing coalescing and big blocks.
    EditText { id: BlockId, range: Range<usize>, removed: String, inserted: String },
    /// Re-parent the children of `from` under `to` (used by merges / direct outdenting).
    AdoptChildren { from: BlockId, to: BlockId, at: usize, moved: Vec<BlockId> },
    /// Page-level.
    SetPreamble { page: PageKey, before: Option<String>, after: Option<String> },
    CreatePage { page: PageKey, title: String, path: Option<RelPath> },
    DeletePage { page: PageKey, captured: Option<Box<PageSnapshot>> },
    RenameFile { page: PageKey, from: RelPath, to: RelPath },
}

pub struct Position { pub page: PageKey, pub parent: Option<BlockId>, pub index: usize }

impl Op {
    pub fn apply(&mut self, g: &mut Graph) -> Result<(), OpError>; // fills `captured`/`removed`
    pub fn inverse(&self) -> Op;
}
```

Invariants checked after every transaction (debug builds; cheap version in release): the tree is acyclic, each block
has exactly one parent slot, uuids are unique per graph, and `Move` never targets the moved block's own descendant.
Logseq checks the equivalent `(left,parent)` uniqueness in `src/main/frontend/modules/outliner/datascript.cljc:164-174`.

### 3.2 Commands (user intents → ops)

| Command | Ops produced | Logseq reference |
|---|---|---|
| `SplitBlock { id, cursor }` (Enter) | `SetText(id, head)`, `InsertSubtree(new tail block)` as first child if `id` has expanded children, else next sibling. Cursor at 0 with text after it → insert an empty block **before**. | `editor.cljs:421`, `:429-455`, `:464-511`, `:536-569` |
| `NewLine` (Shift+Enter) | buffer-only | `editor.cljs:2281` |
| `OutdentEmptyLast` (Enter on empty last child) | `Move` | `editor.cljs:2240-2248`, `:2504-2508` |
| `MergeWithPrevious { id }` (Backspace @0) | `SetText(prev, prev+text)`, `AdoptChildren(id→prev)`, `RemoveSubtree(id)`. If `id` has a uuid that is referenced: the survivor takes over `id::` (or refs are rewritten; see Open questions). Refused if both have children (Logseq rule). | `editor.cljs:790-869` |
| `MergeNext { id }` (Delete @end) | symmetric | `editor.cljs:2659-2702` |
| `Indent { ids }` | `Move` as last child of the previous sibling; also un-collapse that sibling (`SetText` that removes `collapsed::`) | `modules/outliner/core.cljs:802-833` |
| `Outdent { ids, logical }` | `Move` after the parent. In direct mode (default), the following siblings also become children of the moved block (`Move`s). | `core.cljs:834-854` |
| `MoveUpDown { ids, up }` | `Move` (crosses parent boundaries like Logseq) | `core.cljs:760-800` |
| `SetCollapsed { ids, bool }` | `EditText` adding/removing `collapsed:: true` | `editor.cljs:3482-3499` |
| `CycleMarker { ids }` | `EditText` on the marker word (TODO/DOING/DONE or LATER/NOW/DONE) | `src/main/frontend/util/marker.cljs:40-73` |
| `SetHeading { ids, Auto/H1..H6/None }` | `EditText` (`#` prefix or `heading:: true`) | `editor.cljs:3822-3885` |
| `SetProperty { id, key, value }` | `EditText` on the property line (append if missing) | `src/main/frontend/handler/editor/property.cljs:76-122` |
| `EnsureUuid { id }` (copy ref/embed, `((` pick, Alt-drop) | `EditText` inserting `id:: <uuid>` after the title line | `editor.cljs:955-973`, `:1944-1966` |
| `InsertBlocks { target, sibling, blocks, keep_uuids }` (paste, template) | `InsertSubtree`×n (+ `RemoveSubtree` of an empty target) | `core.cljs:520-593`, `editor.cljs:2013-2078`, `:2125-2189` |
| `DeleteBlocks { ids }` | `RemoveSubtree`×n + `EditText` in blocks that referenced them (inline the deleted text, Logseq behaviour) | `core.cljs:654-707`, `datascript.cljc:88-119` |
| `MoveBlocks { ids, target, Before/After/Child }` (DnD, cut-paste) | `Move`×n | `src/main/frontend/handler/dnd.cljs:11-58` |
| `RenamePage { from, to }` | `SetPreamble` (title::), `RenameFile`, `EditText` in every referencing block (`[[from]]`, `#from`, `#[[from]]`, property values), across files; merge into an existing page if `to` exists | `src/main/frontend/handler/page.cljs:229-311`, `:450-514`, `:568-645` |
| `MergePages { from, to }` | `Move`×n roots + ref rewrites + `DeletePage(from)` | `page.cljs:568-616` |

Commands are pure functions `fn plan(&Graph, Cmd) -> Result<Vec<Op>, Refusal>` and are easy to unit test against
fixture files. A `Refusal` explains no-ops such as "cannot merge: both blocks have children".

### 3.3 Transactions

```rust
pub struct Transaction {
    pub id: TxId,
    pub label: &'static str,            // "Split block", "Indent", "Typing"…
    pub ops: Vec<Op>,                   // applied ops (with captured data) — inverse = reversed inverses
    pub cursor_before: Option<CursorState>,
    pub cursor_after: Option<CursorState>,
    pub pages: SmallVec<[PageKey; 2]>,  // touched pages → dirty → write queue
    pub at: Instant,
    pub coalesce: Option<CoalesceKey>,  // e.g. (BlockId, TypingRun)
}

pub struct CursorState { pub block: BlockId, pub selection: Range<usize>, pub mode: EditMode }
```

`Graph::commit(cmd)`:
1. flush the active edit buffer (one `EditText`/`SetText`, as Logseq's `save-current-block!` does before every
   structural op, `editor.cljs:1322`);
2. `plan` the command and apply the ops atomically (if any op fails, roll back the applied prefix);
3. push the transaction on the undo stack (coalesced if possible), clear redo;
4. mark pages dirty, notify the index (incremental re-parse of changed blocks only) and the views.

---

## 4. Undo / redo

- One **graph-wide** stack (`Vec<Transaction>`), capped at about 1,000 entries or about 50 MB of captured text.
  Logseq: per-graph datom stacks (`src/main/frontend/modules/editor/undo_redo.cljs:247-272`).
- **Typing coalescing:** consecutive `EditText`s on the same block merge into one entry while all of these hold:
  no structural op in between, less than 1.5 s between keystrokes, and no word-boundary-after-pause. Logseq's
  granularity is its 500 ms auto-save (`editor.cljs:1861`), which feels too fine.
- Undo applies `ops.iter().rev().map(inverse)` and restores `cursor_before`. Redo re-applies and restores
  `cursor_after`. Logseq restores the editor cursor the same way (`src/main/frontend/handler/history.cljs:10-19`).
- Undo and redo are transactions on the write path too: touched pages become dirty and are written.
- External reloads (§6.3) do **not** clear history. Ops address `BlockId`s, which reload remaps. An op whose target
  vanished makes undo stop with a notice ("history truncated by external change").
- Optional later: a per-page undo mode, like Logseq's `:history/page-only-mode?` (`undo_redo.cljs:150-156`).

---

## 5. Serialization and write path

### 5.1 Span-preserving serializer

```text
serialize(page):
  out = preamble.verbatim_or_rendered()
  for block in dfs(page.roots):
      if block.is_clean():                       // §2.2
          out += disk.bytes[block.origin.span]   // includes its own trailer (blank lines)
      elif let Some(o) = block.origin and text unchanged:   // depth changed only
          out += reindent(disk.bytes[o.span], o.depth → block.depth, page.style)   // only leading prefixes change
      else:
          out += render(block.text, depth, page.style)   // bullet + text, continuation lines indented
  normalize the junction between consecutive emitted chunks (exactly one line ending; keep final_newline)
```

- `render` follows Logseq's `transform-content` rules (`src/main/frontend/modules/file/core.cljs:34-90`). The
  line is `indent×depth` + `- ` + the first line, and continuation lines get `indent×depth + "  "`. One difference:
  `indent` is the **detected** `FileStyle.indent_unit`, not Logseq's global `:export/bullet-indentation`. A brand-new
  file uses Logseq's default, which is a tab (`src/main/frontend/state.cljs:553-563`).
- Special cases kept from Logseq: the pre-block or front matter is written without a bullet. A first top-level
  Markdown heading block whose source had no bullet keeps having none.
- **Round-trip invariant (tested):** `serialize(parse(bytes)) == bytes` for every file in a corpus. The corpus
  includes real graphs, Logseq's own test fixtures, mixed tabs and spaces, `*`/`+` bullets, CRLF, a missing final
  newline, and code fences containing `- `.
- **Self-check before writing:** parse the output and compare the block tree (depths + texts) with the model. On
  mismatch, fall back to a full canonical render of the page and log a bug report. This never writes something that
  re-parses differently.

### 5.2 Write queue

```rust
pub struct WriteQueue { pending: BTreeMap<PageKey, Instant>, debounce: Duration /* 400 ms */, max_delay: Duration /* 2 s */ }
```

1. A dirty page is scheduled `debounce` after its last change, but no later than `max_delay` after the first
   unwritten change. Logseq batches every 1 s (`src/main/frontend/modules/outliner/file.cljs:17`) and defers pages
   over 500 blocks until 3 s idle (`:55-59`). Span-preserving serialization is O(changed bytes + spans), so the
   idle rule is not needed.
2. Writes run on a background executor. The UI never blocks on I/O.
3. **Pre-write check:** `stat` the file. If `(len, mtime)` differ from `disk`, read the file and compare its
   `blake3` hash with `disk.hash`.
   - Equal: write.
   - Different: run the **external-change path** (§6.2) before writing.
   - Missing: if the file was deleted externally and the page is dirty, recreate it and notify.
4. **Atomic write:** write to `.<name>.bitacora-tmp` in the same directory, `fsync`, then `rename` over the target.
   Preserve permissions. Fall back to an in-place write on filesystems where rename fails (some network or FUSE
   mounts).
5. After writing, set `disk = {bytes, hash, mtime}` and **rebase origins**: every emitted block gets a fresh
   `Origin` pointing into the new bytes (the serializer records offsets as it emits). The page becomes clean.
6. Record `(path, hash)` in a short-lived **echo filter** so the watcher event for our own write is ignored
   (Logseq relies on trimmed-content equality, `src/main/frontend/fs/watcher_handler.cljs:91-94`).
7. Optional: back up the previous bytes to `logseq/bak/<path>/<ISO>.Desktop.md` when the write deletes text, keeping
   6 versions like Logseq (`src/electron/electron/backup_file.cljs:27-51`, `src/electron/electron/handler.cljs:70-80`).
   See Open questions.

### 5.3 Multi-file transactions

`RenamePage`/`MergePages`/`DeleteBlocks` with references can touch many pages. All of them are marked dirty in one
transaction and flushed together. `RenameFile` runs first, and the new path is used by the write. A failure on one
file shows an error with a list of the files that were not written. The in-memory state stays authoritative and is
retried. Partial on-disk application is unavoidable without a journal, which is acceptable for MVP.

---

## 6. Concurrency with the outside world

### 6.1 Sources of external change

Logseq itself on the same graph, git pull/checkout, Syncthing/iCloud/Dropbox, and text editors. The watcher
(see [[01-file-graph-layout]]) delivers path events. Bitacora debounces them for 100 ms and then reads and hashes the
file.

### 6.2 Block-level 3-way merge

When a dirty page meets changed disk content (pre-write check or watcher):

```text
base   = parse(page.disk.bytes)          // what both sides started from
theirs = parse(new disk bytes)
ours   = page (in memory)
align blocks base↔theirs and base↔ours: by uuid (id::) first, then by (parent path, text) LCS on the DFS sequence
if the changed block sets are disjoint and structural changes do not conflict → apply theirs' changes as ops onto ours
   (they become a non-undoable "External change" transaction), then write normally
else → conflict: keep ours in memory, show a non-modal banner "Page changed on disk" with
   [Keep mine (overwrite)] [Take disk version] [Show diff] (per-block conflict markers later)
```

The merge itself is `bitacora_merge::merge_page` from the `bitacora-merge` crate, shared with git sync
(ADR-016); core only adapts its result into ops. The base is the in-memory `DiskSnapshot.bytes`; there is no
on-disk snapshot store (no `file_snapshots` table, ADR-017). After a restart there is no base for a file that was not
loaded/touched since: an external change with no pending local edits is simply reloaded; if local edits are pending,
Bitacora falls back to a 2-way per-block diff (disk vs ours) surfaced in the "Page changed on disk" notice.

Logseq simply refuses to write and opens a diff modal (`src/main/frontend/handler/events.cljs:374-380`,
`src/main/frontend/components/diff.cljs:27`). The merge above handles the common case silently: Logseq or git
touched a *different* block.

### 6.3 Reload of a clean page

If a page is not dirty, Bitacora re-parses it and **remaps `BlockId`s** using the same alignment, so the view keeps
scroll position, selection and UI-only state. If the block being edited changed on disk, Bitacora keeps the edit
buffer, marks the block "conflicted", and offers a choice when the edit is committed. Logseq has no protection here
(`watcher_handler.cljs:44-56` re-parses and can drop in-progress text).

---

## 7. Editing UI in GPUI

### 7.1 Page view

- `PageView` (an `Entity`) owns a flattened **row list**: a DFS of visible blocks that skips the children of
  collapsed blocks. It is rendered with `gpui::list` + `ListState`, which handles variable-height virtualization.
  Logseq only does incremental "infinite" loading of 50 + 25 blocks (`src/main/frontend/db/model.cljs:28-30`).
  Row heights are measured lazily, and `ListState::splice` is applied per transaction (only affected ranges).
- Row = gutter (collapse arrow, bullet with drag handle, children-count badge for collapsed blocks) + content.
  Indentation guides are drawn per depth.
- Content is **either** the rendered block (inline runs built from the block AST, see [[03-parsing-indexing-search]])
  **or** the `BlockEditor` for the one block in edit mode.

### 7.2 BlockEditor

- A custom multi-line text element implementing `EntityInputHandler` (IME, marked text, `replace_text_in_range`).
  It uses `TextLayout`/`WrappedLine` for soft wrap and caret geometry. It edits `EditProjection.visible`.
- **Click to caret:** the inline renderer records, for every rendered run, its source byte range in `visible`.
  A click on rendered text maps the glyph index to the source offset. Clicks on hidden markup such as `[[`/`]]`
  snap to the nearest offset. This is Bitacora's version of Logseq's `caret-range` hack
  (`src/main/frontend/components/block.cljs:2200-2203`).
- Vertical navigation: ↑ on the first visual row / ↓ on the last row moves to the previous/next visible block, keeping
  the x position (Logseq `editor.cljs:2558-2605`). ← at 0 and → at end move to the neighbour's end/start
  (`:2607-2653`).
- Buffer flush (commit to model) happens 500 ms after the last keystroke, and on blur, Esc, block navigation,
  window deactivate, and before any command. Same triggers as Logseq (`editor.cljs:1852-1868`,
  `src/main/frontend/handler/editor/lifecycle.cljs:35-44`).
- Autopair: `[] {} () `` ~~ ** __ ^^ == ++` with Logseq's rules (`editor.cljs:1565-1616`, `:2849-2948`), including
  "typing the closing char skips over it" and "Backspace deletes the pair".

### 7.3 Modes and key contexts

GPUI key contexts mirror Logseq's shortcut handler groups:

| Context | Active when | Examples |
|---|---|---|
| `Outliner` | page focused, nothing edited | `Enter` edit selected, `Esc` clear selection, `t o`, `g j` |
| `BlockSelection` | ≥1 block selected | `Backspace` delete, `Tab`, `Mod+C/X`, `Mod+Enter`, `Alt+Shift+↑/↓`, `Shift+↑/↓` extend |
| `BlockEditor` | editing | `Enter`, `Shift+Enter`, `Tab`, `Backspace`, `Mod+Enter`, `Mod+↑/↓`, `Mod+.`/`Mod+,`, `Esc` → select block |
| `Autocomplete` | popup open (higher precedence) | `Enter`, `Shift+Enter`, `↑/↓`, `Ctrl+P/N`, `Esc` close popup only |

Default bindings come from the table in [[04-editor-outliner-operations]] §7 and are overridable through a keymap file.

### 7.4 Autocomplete

`EditorAction` state: `PageSearch { start, hashtag }`, `BlockSearch { start }`, `Slash { start }`,
`AngleCommands { start }`, `PropertyKey { start }`, `PropertyValue { key, start }`, `DatePicker { purpose }`,
`TemplateSearch`. Triggers follow `handle-last-input` (`editor.cljs:1875-1936`): `[[` after autopair, `#` at line
start or after whitespace, `((`, `/` at line start or after whitespace, `<`, and `::` at line start. The popover is
anchored to the caret rect and fed by the search index. Like Logseq, the query is the text between `start` and the
caret, and choosing an item replaces that range. Commands are data (`Vec<Step>`), modelled on Logseq's step vectors
(`src/main/frontend/commands.cljs:218-313`, `:724`), so users can extend them later.

### 7.5 Selection, drag and drop, clipboard

- Selection is a set of `BlockId`s plus an anchor. Shift+click selects a range in DFS order and Mod+click toggles.
  Operations act on the **top-level** blocks of the selection, like `get-top-level-blocks` in Logseq.
- Drag from the bullet. Drop zone rules match Logseq (`components/block.cljs:2623-2643`): near the top edge → before,
  x offset > 50 px → child, otherwise → after. Alt-drop inserts `((ref))` and runs `EnsureUuid`.
- Copy writes three formats: plain Markdown (re-indented subtree, `id::` stripped as Logseq does), HTML, and a private
  MIME type carrying the subtree with uuids. Paste of the private format keeps uuids only after a *cut*. Plain-text
  paste that matches `^\s*([-+*]|#+)\s+` is parsed into a subtree. Text with blank-line-separated paragraphs becomes
  sibling blocks, and anything else is inserted inline (`src/main/frontend/handler/paste.cljs:23-47`, `:101`,
  `:118-177`).

---

## 8. Rendering (non-edit)

Inline runs: text, emphasis, code, `[[page]]`/`#tag` (clickable, hover preview later), `((ref))` (resolved title,
live-updating), links, images (async load), and math (later). Block decorations: marker checkbox (click = `CycleMarker`
to DONE/TODO), priority badge, SCHEDULED/DEADLINE chips, a properties table that filters hidden keys, and a collapsed
LOGBOOK. Embeds `{{embed ((uuid))}}` / `{{embed [[page]]}}` render nested outlines that can be edited in place, with a
cycle guard and a depth limit. `{{query …}}` renders a placeholder in MVP and results later.

Journals view: a virtual list of journal pages, newest first, loading 7 more on scroll (Logseq
`src/main/frontend/handler/page.cljs:676-679`). Today's journal is a **virtual page** (no file) with the default
template applied in memory. Its first edit creates the file, matching Logseq's "Don't create the journal file until
user writes something" (`src/main/frontend/handler.cljs:60-72`, `src/main/frontend/modules/outliner/pipeline.cljs:12-15`).
Linked references are grouped by page with breadcrumbs. Unlinked references are computed on expand.

---

## 9. Feature scope

### MVP

1. Open page → render the outline; virtualized list; collapse arrows honouring `collapsed:: true`.
2. Click-to-edit with caret placement; raw-text editing with the hidden-property projection; IME support.
3. Debounced commit (500 ms), commit on blur/Esc/navigation.
4. Enter split (with first-child rule), Shift+Enter, Enter on empty last child = outdent.
5. Backspace@0 merge-with-previous, Delete@end merge-next (Logseq refusal rules).
6. Tab / Shift+Tab (direct outdenting), Alt+Shift+↑/↓ move.
7. Collapse/expand (Mod+↑/↓, click arrow) persisted as `collapsed:: true`.
8. Block selection (Esc, Shift+↑/↓, Shift+click) with delete/indent/outdent/move/copy/cut.
9. Copy/cut/paste of subtrees; paste of multi-line Markdown → blocks.
10. Cycle TODO (Mod+Enter) and checkbox click.
11. `[[` and `#` page autocomplete (creating pages on demand); `((` block search with `EnsureUuid`; copy block ref
    (Mod+C while editing with no selection, as in Logseq).
12. Undo/redo with typing coalescing and cursor restore.
13. Span-preserving serializer with round-trip tests and the self-check fallback.
14. Debounced write queue, atomic writes, hash-based external-change detection, watcher echo filter, and a
    reload-or-conflict banner (simple "keep mine / take disk").
15. Journals view with a virtual today's journal; page view; zoom into block (Mod+. / Mod+,) with breadcrumb.
16. Rendering of refs, tags, block refs, markers, properties, basic Markdown inlines.

### Later

- Block-level 3-way auto-merge (MVP ships detection + banner only).
- `/` slash commands (full catalogue), `<` block commands, date picker, SCHEDULED/DEADLINE editing.
- `::` property key/value autocomplete; templates (`template::`, dynamic variables, default journal template).
- Drag & drop (incl. Alt-drop ref), multi-select via Mod+click.
- Page rename with reference rewriting and file rename; page merge; page delete.
- Delete-with-references inlining (Logseq `replace-ref-with-content`).
- Embeds edited in place; `{{query}}` and advanced query results; linked / unlinked references panel with filters.
- Right sidebar (Shift+click to open), document mode (`t d`), number lists (`logseq.order-list-type`), headings menu.
- Time tracking LOGBOOK on NOW/DOING; repeated tasks.
- Org-mode editing; `logseq/bak` backups; per-page undo mode; customizable keymap UI.

---

## Requirements for Bitacora

1. **MUST** keep each page as a tree of blocks with session-stable `BlockId`s and, for blocks loaded from disk, the
   byte `Origin` span of their own lines.
2. **MUST** emit clean blocks verbatim. A block whose only change is depth **MUST** only have its leading indentation
   prefixes rewritten.
3. **MUST** satisfy `serialize(parse(bytes)) == bytes` for all files in the compatibility corpus, and **MUST** verify
   each serialized page by re-parsing before writing (with a canonical-render fallback).
4. **MUST** express every mutation as invertible `Op`s grouped into `Transaction`s. One user action is one undo step.
5. **MUST** flush the active edit buffer before any command, undo, navigation or blur.
6. **MUST** preserve hidden property lines (`id::`, `collapsed::`, …) at their original positions when the visible
   text is edited.
7. **MUST** write files atomically, hash-check the disk file before each write, and never overwrite unseen external
   changes.
8. **MUST** ignore watcher events caused by its own writes (hash echo filter).
9. **MUST** create `id:: <uuid>` on a block when a reference or embed to it is created, and keep references valid
   through merges.
10. **MUST** virtualize the page list so editing stays responsive on pages with ≥ 5,000 blocks.
11. **MUST** use the file's detected indentation unit and line endings for newly emitted lines (tab and LF for new
    files).
12. **SHOULD** auto-merge non-overlapping external changes at block granularity, and keep in-progress edits when the
    edited block changes on disk.
13. **SHOULD** coalesce typing into word/pause-sized undo steps.
14. **SHOULD** implement commands as pure `plan(&Graph, Cmd) -> Vec<Op>` functions with fixture-based tests that
    replay Logseq behaviours documented in [[04-editor-outliner-operations]].
15. **SHOULD** support multi-file transactions (rename/merge) with an error report listing unwritten files.
16. **SHOULD** expose the Logseq default keymap with overridable bindings and the four key contexts of §7.3.

## Open questions

- Merge with a referenced block: adopt the `id::` like Logseq (survivor takes the deleted block's uuid), or rewrite
  `((uuid))` in referencing files? Adoption avoids touching other files but changes the identity of the survivor.
- Should Bitacora ever canonicalize a file (an explicit "Reformat page" command), or always stay byte-preserving?
- How should blank lines between blocks be treated when a block is moved? Currently they travel with the preceding
  block's trailer.
- Debounce values (400 ms / 2 s max) versus Logseq's 1 s batch: do we need to be slower to avoid fighting a running
  Logseq instance on the same graph?
- Is `logseq/bak/` interoperability worth it, or should Bitacora rely on git / its own history directory?
- Granularity of the 3-way merge when both sides edited the *same* block: line-level merge inside the block, or
  always a conflict?
- Should undo history persist across restarts (e.g. per-graph journal file)?
- Org-mode: render-only in MVP. When editing arrives, can the same span model handle `*`-depth headlines?
