# Logseq file-based graph: on-disk layout and file/path conventions

> Source analysed: `/www/Bitacora/logseq` at commit `03bcefbdf8` (14 Nov 2025). The snapshot is the
> **0.10.x file-graph release line** (`resources/package.json` → `"version": "0.10.15"`). There is no DB-graph
> code on this branch, so everything below describes the Markdown/Org file graph. All references are
> `path:LINE` relative to the Logseq repo root.
>
> Sibling documents: [[02-markdown-block-syntax]] (block/property syntax inside a file),
> [[03-parsing-indexing-search]] (how files become the in-memory DB), [[04-editor-outliner-operations]]
> (how edits are serialised back to files), [[05-git-and-apis]].

## Summary

A Logseq graph is a plain directory. Pages live in `pages/`, journals in `journals/`, binary files in
`assets/`, Excalidraw drawings in `draws/`, whiteboards (EDN) in `whiteboards/`, and app metadata in
`logseq/` (`config.edn`, `custom.css`, `custom.js`, `bak/`, `.recycle/`, `version-files/`). Everything
under `logseq/` except `config.edn`/`custom.css`/`custom.js`/`export.css` is a cache or backup that Logseq
itself ignores when indexing.

There is **no per-file metadata**: a page's identity is derived only from the file's path and contents.
The page title comes from a `title::` property in the first block. If there is none, Logseq decodes the
file name with one of two schemes:

- `:triple-lowbar`, the default in new graphs: `___` stands for `/`, and percent-encoding is used for the
  reserved characters `: * ? " < > | # \`.
- `:legacy`, used when `config.edn` has no `:file/name-format` key: `.` stands for `/`, and the whole name
  is URL-decoded.

Page identity is case-insensitive and NFC-normalised (`:block/name` = lower-cased title). Journal pages are
recognised by whether the title parses as a date, not by their directory. Their files are named
`yyyy_MM_dd.md` by default, and the display title is re-rendered with `:journal/page-title-format`.

Several things live outside the graph folder: a DB cache in `~/.logseq/graphs/*.transit`, the global
config, plugins, a separate git dir, the search index and localStorage. Bitacora can ignore all of them
except the global `~/.logseq/config/config.edn`, which is optional.

---

## 1. Graph directory structure

| Path (relative to graph root) | Created by Logseq | Indexed? | Purpose / notes |
|---|---|---|---|
| `pages/` | yes (`handler/repo.cljs:52,76`) | yes | Non-journal pages. Configurable with `:pages-directory`. Sub-directories are allowed and indexed (`common/graph.cljs:44-64` walks recursively). |
| `pages/contents.md` | yes, on new graph (`handler/repo.cljs:40-57`); default content is `-` (`src/resources/templates/contents.md`) | yes | Special page: any file whose path starts with `pages/contents.` gets the title `Contents` (`extract.cljc:45-46`). |
| `journals/` | yes (`handler/repo.cljs:119-124`) | yes | Daily pages, named `yyyy_MM_dd.<ext>` by default. Configurable with `:journals-directory`. |
| `assets/` | on first paste (`handler/editor.cljs:1392-1397`) | **no** (binary; only text extensions are read) | Pasted or dropped files. Linked from pages as `../assets/<name>`. |
| `draws/` | on first `/Draw` (`handler/draw.cljs:14-40`) | listed (`.excalidraw` is an allowed format) but not parsed into blocks | Excalidraw JSON files named `yyyy-MM-dd-HH-mm-ss.excalidraw`. |
| `whiteboards/` | on first whiteboard | yes, as EDN (`graph_parser.cljs:103-104`, `extract.cljc:248-280`) | tldraw whiteboards, one `.edn` file per whiteboard page. Configurable with `:whiteboards-directory`. |
| `logseq/config.edn` | yes (`handler/repo_config.cljs:38-51`) | parsed as config, also stored as a `:file` entity | Graph config. Template: `src/resources/templates/config.edn`. |
| `logseq/custom.css` | yes, empty (`handler/repo.cljs:59-69`) | stored as file content | User CSS. In it, `../assets/` is rewritten to the assets URL (`config.cljs:461-478`). |
| `logseq/custom.js` | no (user-created) | read on demand | User JS, run after a confirmation prompt (`handler/ui.cljs:128-160`). |
| `logseq/export.css` | no | — | CSS for exports (`config.cljs:347,454-459`). |
| `logseq/.recycle/` | yes (`handler/repo.cljs:123`) | **ignored** | Deleted page files go here, flattened (see §10). |
| `logseq/bak/` | on demand | **ignored** | Automatic backups before overwriting or after conflicts (see §11). |
| `logseq/version-files/` | Logseq Sync only | **ignored** | Sync file history (`local/` = local versions). |
| `logseq/graphs-txid.edn` | Logseq Sync only | **ignored** | Sync metadata (`[...uuid txid]`). |
| `logseq/pages-metadata.edn`, `logseq/metadata.edn` | very old versions | **ignored** (`common/graph.cljs:89`) | Legacy page timestamps. No longer written. |
| `.git/` (dir or *file*) | optional | ignored (dot path) | See §13: Logseq can use a separate git dir under `~/.logseq/git/`, leaving a `.git` **file** containing `gitdir: …`. |
| any `.*` path, `node_modules/`, `.DS_Store` | — | ignored | See §1.1. |

### 1.1 Ignore rules

There are two rule sets, applied by different code paths.

1. **Directory walk** (Electron and CLI), `deps/common/src/logseq/common/graph.cljs`:
   - `readdir` (`:44-64`) skips symbolic links and every entry whose name starts with `.`.
   - `ignored-path?` (`:66-96`) ignores relative paths that:
     - start with `.`, `logseq/.recycle`, `logseq/bak` or `logseq/version-files`;
     - are exactly `logseq/graphs-txid.edn` or `logseq/pages-metadata.edn`;
     - contain `/node_modules/`;
     - end with `.DS_Store`;
     - match `/\.[^.]+` or `^\.[^.]+` (any hidden segment).
   - `get-files` (`:98-113`) keeps only these extensions: `org markdown md edn json js css excalidraw tldr`.
2. **Browser/NFS reload path**, `src/main/frontend/util/fs.cljs:20-45`: the same idea, and it additionally
   ignores any file whose extension is not `.md .markdown .org .js .edn .css`.
3. **Parser filter**, `deps/graph-parser/src/logseq/graph_parser.cljs:133-148` (`filter-files`): only
   `:edn :css :org :markdown :md` reach the parser.
   - Order matters: `journals/` files come first (reverse-sorted), then built-ins (`contents.`, `*.edn`,
     `custom.css`), then the rest.
   - This order decides which file wins on duplicate titles (§3.6).
4. **`:hidden` config**, `deps/common/src/logseq/common/config.cljs:5-25`: each pattern is a **path
   prefix** relative to the graph root. A leading `/` is optional and is normalised by prepending `/` to both
   sides. Examples: `"/archived"` hides `archived/**`; `"/test.md"` hides that one file.
   - Applied at load time in `handler/repo.cljs:268-276` and `:290-298`, and in the CLI
     (`graph_parser/cli.cljs:18-35`).
   - It is not a glob: `"archived"` and `"/archived"` are equivalent.
   - The template example `"../assets/archived"` can never match a path, because matching is a plain
     prefix test.

---

## 2. `config.edn`

### 2.1 Location, merging and parsing

- The graph config is `logseq/config.edn` (`config.cljs:345`, `get-repo-config-path` `config.cljs:443-445`).
  It is read with `clojure.edn/read-string` (`handler/repo_config.cljs:21-29`).
  - It must be **valid EDN**. Values such as `(fn [r] ...)` are just lists to EDN.
  - Duplicate keys make the file invalid (`handler/common/config_edn.cljs:66-73`).
- There is also an optional **global config** at `~/.logseq/config/config.edn`
  (`handler/global_config.cljs:19-37`; template `src/resources/templates/global-config.edn`).
- Effective config: `merge-configs(default-config, global, graph)` (`state.cljs:350-383`).
  - Later sources win, except maps, which are shallow-merged.
  - Built-in defaults (`state.cljs:335-345`) are `:feature/enable-search-remove-accents? true`,
    `:ui/auto-expand-block-refs? true`, and **`:file/name-format :legacy`**.
  - So **a config without `:file/name-format` means legacy file naming.**
- Logseq **edits** `config.edn` with `borkdude.rewrite-edn` (`handler/config.cljs:9-31`), which keeps
  comments and formatting. This happens for favorites, `:default-home`, UI toggles and the filename-format
  conversion. Bitacora must also edit it surgically.
- Malli schema of known keys: `src/main/frontend/schema/handler/common_config.cljc:5-95`. Unknown keys are
  allowed.

### 2.2 Keys that affect file layout and parsing

| Key | Default | Effect | Code |
|---|---|---|---|
| `:file/name-format` | absent ⇒ `:legacy`; the new-graph template writes `:triple-lowbar` | Page title ↔ file name codec (§3) | `state.cljs:345,510-513`, `util/fs.cljs:187-196`, `graph_parser/util.cljs:253-258` |
| `:pages-directory` | `"pages"` | Directory for new non-journal pages | `state.cljs:463-468`, `config.cljs:323-325` |
| `:journals-directory` | `"journals"` | Directory for new journal files. The schema misspells it `:journal-directory` (`common_config.cljc:36`). | `state.cljs:470-475` |
| `:whiteboards-directory` | `"whiteboards"` | Directory for whiteboard `.edn` files. Note: `gp-config/whiteboard?` hard-codes `whiteboards/` when parsing (`graph_parser/config.cljs:41,47-51`). | `state.cljs:477-482` |
| `:journal/page-title-format` (legacy alias `:date-formatter`) | `"MMM do, yyyy"` | Display title of journal pages, and the first formatter tried when detecting journals | `graph_parser/config.cljs:70-76` |
| `:journal/file-name-format` | `"yyyy_MM_dd"` | File name of **new** journal files only. Not retroactive (template comment, `config.edn:31-37`). | `date.cljs:195-211` |
| `:preferred-format` | `"Markdown"` (or `:me :preferred_format`) | Extension of new pages (`md`/`org`), bullet style | `state.cljs:447-457` |
| `:hidden` | `[]` | Path-prefix exclusions (§1.1) | `common/config.cljs` |
| `:default-templates {:journals "name"}` | `""` | Template inserted into a new journal page | `state.cljs:427-431`, `handler/repo.cljs:89-100` |
| `:feature/enable-journals?` | `true` | When `false`, no journal file is auto-created | `state.cljs:609-613` |
| `:feature/enable-whiteboards?` | `true` | UI only | — |
| `:property-pages/enabled?` | `true` (template) | Property keys become pages (DB only, no files) | template `config.edn:320` |
| `:property-pages/excludelist` | — | Keys not turned into pages | — |
| `:property/separated-by-commas` | — | Extra property keys whose comma-separated values become page refs. `alias`, `aliases` and `tags` are always comma-separated (`graph_parser/text.cljs:141-146`). | see [[02-markdown-block-syntax]] |
| `:ignored-page-references-keywords` | — | Property keys whose values are never parsed for refs (`text.cljs:165-175`) | — |
| `:block-hidden-properties` | — | UI hiding only | `config.cljs:493-495` |
| `:export/bullet-indentation` | `:tab` | Indentation **used when writing files** (`:tab`, `:two-spaces`, `:four-spaces`, `:eight-spaces`) | `state.cljs:551-…`, `modules/file/core.cljs:72-75` |
| `:org-mode/insert-file-link?` | `false` | Org only: insert `[[file:../pages/x.org][x]]` links instead of `[[x]]` | `handler/page.cljs:685-711` |
| `:favorites` | `[]` | Page names (strings). Rewritten on rename/delete. | `handler/page.cljs:313-340` |
| `:default-home` | — | `{:page "X" :sidebar ...}`. Rewritten on rename. | `handler/page.cljs:487-489` |
| `:preferred-workflow` | `:now` | `NOW/LATER` vs `TODO/DOING` for new tasks | `state.cljs:497-…` |
| `:file-sync/ignore-files` | `[]` | Regexes excluded from Logseq Sync | `fs/sync.cljs:176-191` |
| `:meta/version` | `1` | Informational | — |

### 2.3 Default template (`src/resources/templates/config.edn`, 421 lines)

These keys are **active** in the template:

- `:meta/version 1`
- `:preferred-workflow :now`
- `:hidden []`
- `:default-templates {:journals ""}`
- `:ui/enable-tooltip? true`, `:ui/show-full-blocks? false`, `:ui/auto-expand-block-refs? true`
- `:feature/enable-block-timestamps? false`, `:feature/enable-search-remove-accents? true`
- `:start-of-week 6`
- `:shortcuts {}`, `:shortcut/doc-mode-enter-for-new-block? false`
- `:block/content-max-length 10000`
- `:ui/show-command-doc? true`, `:ui/show-empty-bullets? false`
- `:query/views {:pprint (fn …)}`, `:query/result-transforms {:sort-by-priority (fn …)}`
- `:default-queries {:journals [NOW query, NEXT query]}`
- `:commands []`
- `:outliner/block-title-collapse-enabled? false`
- `:macros {}`
- `:ref/default-open-blocks-level 2`, `:ref/linked-references-collapsed-threshold 50`
- `:favorites []`
- `:property-pages/enabled? true`
- `:file/name-format :triple-lowbar`

Everything else is present only as a comment, including `:preferred-format`, `:journal/page-title-format`,
`:journal/file-name-format`, `:pages-directory`, `:journals-directory` and `:whiteboards-directory`.

Logseq Sync treats a `config.edn` whose content equals this template (MD5 check, `config.cljs:349-353`) as
"default". **Bitacora should embed this exact file** when creating a new graph, so that Logseq sees an
identical, untouched config.

---

## 3. Page title ↔ file name mapping (critical)

### 3.1 How a title is derived from a file (read path)

`extract.cljc:30-63` (`get-page-name`) applies these rules **in priority order**:

1. If the path starts with `pages/contents.` → `"Contents"` (hard-coded; `:pages-directory` is not
   consulted).
2. **`title::` property**: if the first AST element of the file is `Properties` or `Property_Drawer`, its
   `title` key (case-insensitive) is the page title.
   - In Markdown this means the leading property lines before the first bullet (`title:: My Page`), or YAML
     front matter (`---\ntitle: My Page\n---`).
   - In Org it means `#+TITLE:` or a top `:PROPERTIES:` drawer.
3. **File name**: take `path->file-body` (the base name with everything after the **last** dot removed,
   `graph_parser/util.cljs:204-209`) and decode it with `title-parsing` (`util.cljs:253-258`). This only
   happens for mldoc formats (`md`/`markdown`/`org`).
4. The text of the first heading block (practically unreachable, because step 3 always yields a name).

After that, `gp-block/convert-page-if-journal` (`block.cljs:273-284`) checks whether the title is a date
and, if so, rewrites it (§4).

The derived names are:

- `:block/original-name` = the derived title, with case preserved.
- `:block/name` = `page-name-sanity-lc` = `lower-case` + one leading and one trailing `/` removed + Unicode
  **NFC** (`util.cljs:134-142,162-165`).

Page identity is therefore **case-insensitive and NFC-normalised**.

### 3.2 `:triple-lowbar` format (default for new graphs)

**Encoding** (title → file body): `tri-lb-file-name-sanity`, `src/main/frontend/util/fs.cljs:125-135`.
Each step runs on the output of the previous one, in this order:

| # | Step | Code |
|---|---|---|
| 1 | `page-name-sanity`: strip **one** leading and **one** trailing `/`, then NFC-normalise. Case is **preserved**. | `graph_parser/util.cljs:134-142` |
| 2 | Pre-escape existing percent sequences: every `%[0-9a-fA-F]{2}` becomes `%25XX`. A bare `%` (e.g. `50% done`) is left alone. | `fs.cljs:90-92,131` |
| 3 | Each run of reserved chars ``[: * ? " < > \| # \\]`` is replaced by `encodeURIComponent(run)` with `*`→`%2A`: `:`→`%3A` `*`→`%2A` `?`→`%3F` `"`→`%22` `<`→`%3C` `>`→`%3E` `\|`→`%7C` `#`→`%23` `\`→`%5C` | `fs.cljs:76-79,118-123,132` |
| 4 | A leading `.` becomes `%2E`, so the file is not a hidden file. | `fs.cljs:133` |
| 5 | If the whole body is a Windows reserved name (`CON PRN AUX NUL COM1-9 LPT1-9`, **case-sensitive**) or ends with `.`, append `/` (which becomes `___` in step 6). | `fs.cljs:106-116,134` |
| 6 | Disambiguate underscores, then encode namespaces: `___`→`%5F%5F%5F`, `_/`→`%5F/`, `/_`→`/%5F`, then every `/`→`___`. | `fs.cljs:94-104,135` |

Not encoded: spaces, Unicode, `.` (except a leading one), `,`, `'`, `!`, `&`, `+`, `[`, `]`, `(`, `)`, `{`,
`}`, `=`, `;`, `@`, `$`, `~`, `` ` ``, and `%` when it is not followed by two hex digits. There is **no
length limit** and **no lower-casing**.

**Decoding** (file body → title): `tri-lb-title-parsing`, `graph_parser/util.cljs:153-160`:

1. Replace every `___` with `/`. A run of 4–5 underscores is replaced left-to-right; for example `____`
   becomes `/_`.
2. Replace each individual `%XX` token (case-insensitive) with `decodeURIComponent(token)`.
   - Tokens that cannot be decoded on their own (e.g. `%E4` from a multi-byte UTF-8 sequence) are left
     unchanged (`util.cljs:10-22`).
   - Decoding is a single pass, so `%252F` → `%2F` and is not decoded again.
3. `make-valid-namespaces`: split on `/`, drop empty segments, join with `/` (`util.cljs:144-149`).

The encoding round-trips for every title that has no empty namespace segments and no leading/trailing `/`.
This is verified by `src/test/frontend/db/name_sanity_test.cljs:9-47`.

**Examples** (computed with a faithful JS port of the functions above):

| Page title | File body (`pages/<body>.md`) | Decoded back |
|---|---|---|
| `Projects/Bitacora/Design` | `Projects___Bitacora___Design` | same |
| `My Page` | `My Page` | same |
| `What? A: B` | `What%3F A%3A B` | same |
| `C#` | `C%23` | same |
| `50% done` | `50% done` | same |
| `a%2Fb` (literal) | `a%252Fb` | same |
| `foo_bar` | `foo_bar` | same |
| `foo___bar` | `foo%5F%5F%5Fbar` | same |
| `a_/b` | `a%5F___b` | same |
| `a/_b` | `a___%5Fb` | same |
| `.hidden` | `%2Ehidden` | same |
| `CON` / `con` | `CON___` / `con` | same |
| `ends with.` | `ends with.___` | same |
| `Version 1.0` | `Version 1.0` | same |
| `v1.0/notes` | `v1.0___notes` | same |
| `a*b`, `tag\|pipe`, `<html>`, `back\slash` | `a%2Ab`, `tag%7Cpipe`, `%3Chtml%3E`, `back%5Cslash` | same |
| `Café` (NFD input) | `Café` (NFC) | NFC |
| `aa?#/bbb/ccc` | `aa%3F%23___bbb___ccc` | same (test `name_sanity_test.cljs:83-86`) |
| `a__/bbb/ccc` | `a_%5F___bbb___ccc` | same (test `:93-96`) |

### 3.3 `:legacy` format (graphs whose config has no `:file/name-format`)

- **Decoding** (`graph_parser/util.cljs:244-247`): replace **every `.` with `/`**, then
  `decodeURIComponent` the whole string, falling back to the raw string on error.
  - `Version 1.0.md` therefore reads as the namespace page `Version 1/0`. This is why legacy graphs depend
    on `title::`.
- **Encoding** (`util/fs.cljs:168-181`, "dir-ver 0 after May 2022"):
  - URL-encode runs of `[\ # | %]`, runs of `[: * ? " < > |]`, and `/` (→ `%2F`);
  - `*` → `%2A`.
  - Example: `Projects/Bitacora` → `Projects%2FBitacora.md`.
- **Even older** `:legacy-dot` scheme (before May 2022, `fs.cljs:143-164`, used only to detect old files
  during conversion): reserved chars become `_` and `/` becomes `.`.
- **`title::` is auto-inserted only in legacy mode.**
  - When a page is created (`handler/page.cljs:95-99`, `fs-util/create-title-property?` `fs.cljs:198-206`),
    Logseq checks whether the title would survive an encode→decode round-trip. If it would not, or if the
    file name would contain reserved chars, Logseq writes `title:: <Original Title>` as the first block.
  - Rename (`page.cljs:483-484`) runs the same check *without* the legacy guard. In triple-lowbar mode the
    check is practically always false.
- Conversion UI: `handler/conversion.cljs` computes renames from legacy to triple-lowbar
  (`calc-rename-target` `:99-129`). Journals are never renamed (`:76-81`). After the user accepts, Logseq
  writes `:file/name-format :triple-lowbar` into `config.edn` (`:10-15`).

### 3.4 `title::` overrides

- When a file has `title::`, that value is the page title, whatever the file name is.
  - A file `pages/foo.md` with `title:: Bar` is page **Bar**, and the file keeps its name until a rename.
  - Bitacora must index by title-from-property first, and only fall back to the file name.
- On rename (`handler/page.cljs:450-514`), if the first (properties) block's lower-cased content contains
  the old lower-cased page name, Logseq rewrites `title::` to the new name. In Markdown front matter it
  writes `title: New` (`page.cljs:460-471`, `util/property.cljs:222-224,230-…`).
- Mismatch between `title::` and the file name is tolerated and persists.
- Title values are taken verbatim from the property AST. Quoting/commas are **not** special for `title`
  (`page.cljs:67-71`).

### 3.5 Namespaces

- A title containing `/` is a namespace page (`graph_parser/text.cljs:79-85`). Titles starting with `./` or
  `../`, and URLs, are excluded.
- Parent pages (`a`, `a/b` for `a/b/c`) are created **in the DB only**, with no files
  (`extract.cljc:194-199`, `util.cljs:116-125`). `:block/namespace` points to the direct parent.
- On disk the namespace is flattened into a **single file** in `pages/` (`a___b___c.md`, or legacy
  `a%2Fb%2Fc.md` / `a.b.c.md`). Logseq never creates sub-directories for namespaces.
- Files inside sub-directories of `pages/` are still indexed, but the directory **does not** contribute to
  the title. Only the base name does, via `path->file-body`.

### 3.6 Case-insensitivity and collisions

- `:block/name` is lower-cased, so `pages/Foo.md` and `pages/foo.md` (on a case-sensitive FS), or two files
  whose titles differ only in case, collide.
- At load time the **first** file in `filter-files` order wins. Logseq skips the rest and shows "The file X
  will be skipped because another file Y has the same page title" (`handler/repo.cljs:216-238`).
- At runtime, a second file mapping to an existing page is an error, unless the paths differ only in case
  (case-only rename on case-insensitive filesystems), in which case the DB path is updated and the old
  content backed up (`handler/common/file.cljs:24-49`).
- The file name keeps the case of `:block/original-name` at creation time
  (`modules/file/core.cljs:127-130`).

### 3.7 Path normalisation

- Every path is NFC-normalised before it is stored or written (`graph_parser/util.cljs:24-28`,
  `frontend/fs.cljs:92-97,126-139`, `common/path.cljs:206-212`).
- Paths stored in the DB are **relative to the graph root** with `/` separators (e.g. `pages/foo.md`,
  `logseq/config.edn`). On Windows, `\` is converted to `/` (`common/graph.cljs:36-42`).

---

## 4. Journal files

| Aspect | Rule | Code |
|---|---|---|
| Directory | `:journals-directory`, default `journals/` | `config.cljs:327-329` |
| File name | `tf/unparse(:journal/file-name-format or "yyyy_MM_dd", date)` + `.md`/`.org` → e.g. `journals/2025_11_14.md` | `date.cljs:195-211`, `handler/page.cljs:835-837`, `modules/file/core.cljs:124-136` |
| Is-journal detection | **By title, not by directory.** The page title (from `title::` or the file name) is passed through `capitalize-all` and parsed with `[:journal/page-title-format, "MMM do, yyyy", "yyyy-MM-dd", "yyyy_MM_dd"]` (`date_time_util.cljs:15-41`). If any formatter parses it, the page is a journal. | `block.cljs:273-284` |
| Display title | Re-rendered with `:journal/page-title-format` (default `MMM do, yyyy` → `Nov 14th, 2025`). `:block/name` is its lower-case form (`nov 14th, 2025`). | `block.cljs:280-281`, `date_time_util.cljs:48-51` |
| `:block/journal-day` | Integer `yyyyMMdd`, e.g. `20251114` | `date_time_util.cljs:35-41` |
| References | `[[Nov 14th, 2025]]` (in the configured title format) resolves to the journal page | — |

Consequences:

- A file `pages/2024_01_01.md` is also treated as a journal.
- A custom `:journal/file-name-format` other than `yyyy_MM_dd` / `yyyy-MM-dd` (e.g. `yyyyMMdd`) is **not**
  in the parse list. Such files are recognised as journals only if the name happens to match
  `:journal/page-title-format`.
- When `:journal/page-title-format` changes, all journal titles and `[[...]]` references written in the old
  format stop matching. Logseq does not rewrite them.

How journal files get created:

- **Today's journal** is created when the app opens, if `:feature/enable-journals?` is on
  (`handler/page.cljs:820-852`, `handler/repo.cljs:80-113`).
  - On a brand-new graph it is written immediately with content `- ` (Markdown) / `* ` (Org), or `- ` +
    the default journal template (`util.cljc:1002-1008`).
  - Otherwise the page is created in the DB, and the file is written once the first block is saved.
  - The file watcher deliberately ignores external "changes" of a journal whose trimmed content equals
    `-`, `*` or the template (`fs/watcher_handler.cljs:91-102`).
- Journals are **never renamed** on disk by a page rename (`page.cljs:486`). Filename-format conversion
  skips them as well.

---

## 5. Assets

- **Location**: always `<graph>/assets/` (hard-coded `"assets"` in `handler/editor.cljs:1392-1403` and
  `graph_parser/config.cljs:16-21`).
- **Name on paste/drop** (`handler/editor.cljs:1405-1454`): the original base name is split into a stem
  and an extension.
  - The extension is `extname`. When `extname` is not a known format, `full-path-extname` (multi-dot) is
    used instead.
  - The stem gets these changes: ` `→`_`, `%`→`_`, `/`→`_`. Then `_<Date.now() ms>_<index>` is appended,
    and runs of `_` are collapsed to one.
  - Examples: `Screen Shot 2024.png` → `assets/Screen_Shot_2024_1731580000000_0.png`. Clipboard image
    `image.png` → `assets/image_1731580000000_0.png`.
  - Older versions used `<page-file-path-with-/→_>_<ms>_<i>.<ext>`, e.g.
    `assets/journals_2021_02_03_1612350230540_0.png` (comment at `editor.cljs:1514`).
- **Link written into the block** (`handler/assets.cljs:117-128`; relative path from
  `editor.cljs:1515-1528`):
  - Markdown image, audio, video or PDF: `![<original file name>](../assets/<name>)`. Other files:
    `[<original file name>](../assets/<name>)`.
  - Org image: `[[../assets/<name>]]`. Other Org files: `[[../assets/<name>][<file name>]]`.
  - The path is **relative to the page file**. For `pages/x.md` and `journals/x.md` it is `../assets/…`;
    for a file in `pages/sub/` it would be `../../assets/…`. If the page has no file yet, the base is the
    dummy path `pages/_.md`.
- **Resolution**: links starting with `../assets`, `./assets`, `/assets` or `assets` count as local assets
  (`local-asset?` regex `^[./]*assets`, `graph_parser/config.cljs:18-21`). The UI resolves them against the
  graph root, not the file directory (`editor.cljs:1456-1468`: leading `../` or `/` is replaced with `./`).
  Bitacora should resolve relative to the file *and* fall back to `<root>/assets`.
- **Asset alias dirs** (`@alias/file.pdf`, `handler/assets.cljs`): external directories mapped by name.
  The configuration is stored **in localStorage** (`state.cljs` `set-assets-alias-dirs!`), not in the
  graph. Bitacora should preserve such links verbatim.
- **Deleting an asset** from a block (`editor.cljs:1495-1512`) calls `fs/unlink!`, which **moves the file
  to `logseq/.recycle/`** (§10). Assets are never deleted otherwise. There is no garbage collection.
- PDF highlights create `hls__<pdf-name>…` pages and `assets/<pdf-stem>/` image folders. They are ordinary
  pages and files, so preserve them.

---

## 6. Whiteboards and drawings

- **Whiteboards**:
  - Format: `whiteboards/<file-name-sanity(title)>.edn` (`modules/file/core.cljs:121-136`).
  - Content: `{:blocks [...] :pages ({...page with :block/properties :logseq.tldraw.page ...})}`,
    printed with `ugly-pr-str` (newline after each map entry) and left-trimmed (`core.cljs:152-156`,
    `modules/file/uprint.cljs`).
  - Parsing: `extract-whiteboard-edn` (`extract.cljc:248-280`). The page name comes from the EDN
    `:block/name`, falling back to the file name.
    - For this fallback `filepath->page-name` (`extract.cljc:21-28`) uses legacy-style decoding
      (`.`→`/` + URL-decode).
  - Blocks carry `:logseq.tldraw.shape` properties.
  - **Bitacora: preserve byte-for-byte; optionally list them as read-only pages.**
- **Draws**:
  - Format: `draws/yyyy-MM-dd-HH-mm-ss.excalidraw` (Excalidraw JSON; template at `handler/draw.cljs:53-56`,
    name at `:58-60` + `date.cljs:75-77`).
  - Embedded in a block as a page ref, `[[draws/2025-11-14-10-30-00.excalidraw]]`
    (`commands.cljs:285-291`).
  - **Bitacora: preserve, and render the ref as a link/attachment.**
- `.tldr` files are listed (`common/graph.cljs:98-99`) but never parsed. Preserve them.

---

## 7. Org-mode vs Markdown

- The format is decided per file by extension: `.md`/`.markdown` → `:markdown`, `.org` → `:org`
  (`graph_parser/util.cljs:188-219`).
- `:preferred-format` only decides the extension of **new** files and the bullet marker (`-` vs `*`,
  `graph_parser/config.cljs:78-85`).
- Both formats are parsed by mldoc and can coexist in one graph. A page backed by `.org` stays `.org`, even
  when the preferred format is Markdown (`modules/file/core.cljs:119-120` uses the page's
  `:block/format`).
- **MVP for Bitacora**:
  - Index `.org` files read-only, using at least the title from `#+TITLE:` or the file name, so that
    references resolve.
  - Never rewrite them.
  - Never create a `.md` file for a page that already has an `.org` file. Otherwise Logseq sees two files
    for one page (§3.6).
- Other extensions found under `pages/` (e.g. `.adoc`) are ignored by the parser and must be left alone.

---

## 8. Encoding, line endings, BOM, front matter

- **Encoding**:
  - Reads use Node `fs.readFileSync(path).toString()`, i.e. UTF-8 (`electron/utils.cljs:220-226`,
    `graph_parser/cli.cljs:13-16`).
  - Writes use `fs.writeFileSync(path, string)`, i.e. UTF-8 (`electron/handler.cljs:117-145`).
  - There is no BOM handling. A leading U+FEFF stays in the content and would end up in the first
    block/property. Only CSV/OPML exports add a BOM (`handler/export.cljs:68`).
- **Line endings**:
  - Logseq always generates `\n`: blocks are joined with `"\n"` (`modules/file/core.cljs:108-110`).
  - CRLF files are read as-is. Mldoc tolerates `\r\n`, and pasted text is normalised
    (`handler/paste.cljs:124`).
  - Once a page is edited, Logseq rewrites the **whole file** with LF.
- **Trailing newline**: the serialised content has **no trailing newline** unless the last block's content
  ends with one. A pre-block (page properties) gets `content.trim() + "\n"`, which produces a blank line
  between page properties and the first bullet (`core.cljs:45-48`). See [[04-editor-outliner-operations]].
- **Content comparison** for conflict detection uses `string/trim` on both sides
  (`fs/node.cljs:16-19`, `fs/watcher_handler.cljs:84-92`), so whitespace-only differences at the ends are
  not treated as changes.
- **YAML front matter**:
  - Markdown files may start with `---\nkey: value\n---`, which mldoc parses into the page `Properties`.
  - Logseq preserves front matter and, when adding a property there, uses `key: value` syntax
    (`util/property.cljs:222-224,282-285`).
  - Logseq never *creates* front matter. New page properties are written as `key:: value` lines.
  - Details are in [[02-markdown-block-syntax]].

---

## 9. Page-level properties

Page properties are the properties of the **first block** of the file when that block is a properties-only
"pre-block" (`extract.cljc:186-193`). In Markdown these are `key:: value` lines at the top of the file,
**without** a leading `- `, as written by `core.cljs:41-48`. Keys are lower-cased when read
(`extract.cljc:228-236`). Keys are valid only if they are valid EDN keywords without `" | ^ ( ) { }` and do
not start with `#` (`graph_parser/property.cljs:22-28`).

| Property | Effect |
|---|---|
| `title::` | Overrides the file-derived title (§3.4). |
| `alias::` (also `aliases::`) | Comma-separated page names. Each alias becomes a DB page with `:block/alias` linking both ways (`extract.cljc:65-103`). **No files are created for alias pages.** An alias equal to the page's own name, or a blank alias, is dropped. |
| `tags::` | Comma-separated page refs → `:block/tags` on the page (`extract.cljc:96-102`). Pages are created in the DB only. |
| `public::` | `true`/`false` (boolean, `property.cljs:79-81`). Controls publishing when `:publishing/all-pages-public?` is false. Toggled by `update-public-attribute!` (`page.cljs:681-683`). |
| `filters::` | Linked-reference filters stored as an EDN map string, e.g. `filters:: {"tag" true, "other" false}`. Written with `pr-str` (`util/page_property.cljs:61`, `page.cljs:748-758`). Backslashes are stripped when parsing (`extract.cljc:238-242`). |
| `icon::` | Emoji/icon shown in the UI. Editable built-in (`property.cljs:58-66`). |
| `template::` (+ `template-including-parent::`) | Marks a **block** (any block, not only the first) as a template named by the value. `:default-templates {:journals "name"}` inserts it into new journals. |
| `exclude-from-graph-view::` | Boolean; graph view only. |
| `collapsed::`, `id::`, `heading::`, `background-color::`, … | Hidden built-ins, mostly block-level (`property.cljs:68-78`); see [[02-markdown-block-syntax]]. |
| Org-only | `#+TITLE:`, `#+ALIAS:`, `#+TAGS:`, `:filetags`, `:macro`. |

The editable built-in set is `title icon template template-including-parent public filters
exclude-from-graph-view logseq.query/nlp-date macro filetags alias aliases tags` plus the `logseq.table.*`
and `logseq.color` keys (`property.cljs:46-66`). Alias, aliases and tags are always comma-separated refs
(`text.cljs:141-146`).

---

## 10. Page creation, rename and delete on disk

### 10.1 Create

`handler/page.cljs:125-178` (`create!`):

- The title is trimmed, `[[ ]]` and leading `#` are removed, and boundary slashes are stripped.
- Namespace parents are created as DB pages without files.
- **No file is written at creation time.** The file path is assigned lazily in
  `transact-file-tx-if-not-exists!` (`modules/file/core.cljs:115-142`), and the file is written only when
  the page's block tree is first saved with non-blank content (`core.cljs:146-166`).
- Path chosen at that moment:
  - journal → `journals/<date→file-name>.<ext>`;
  - whiteboard → `whiteboards/<file-name-sanity title>.edn`;
  - otherwise → `pages/<file-name-sanity(original-name)>.<ext>`.
- Pages that are only *referenced* (`[[New Page]]`) exist in the DB but **never get a file** until someone
  writes content into them.

### 10.2 Rename

`page.cljs:618-645` (`rename!`) → `rename-page-aux` (`:450-514`):

1. **Same lower-cased name (case-only change)**: the page title is updated and the file is renamed.
2. **Target exists** → `merge-pages!` (`:568-616`): the source blocks are moved to the end of the target,
   both files are rewritten, references are updated, and the source page/file is deleted (→ recycle).
3. **Otherwise** → `rename-namespace-pages!` (`:550-566`): renames the page **and all namespace children**
   (`a/x` when `a` is renamed). The old prefix is replaced only once (`string/replace-first`).
4. **Always afterwards** → `rename-nested-pages` (`:516-548`): renames pages whose *titles contain*
   `[[old]]` or `[[old/`.

Per page (`rename-page-aux`):

- The DB name is updated. `title::` is rewritten when the first block mentions the old name (§3.4).
- **The file is renamed** in the same directory with the same extension:
  `compute-new-file-path(old-path, file-name-sanity(new-name))` (`page.cljs:191-199`), done with
  `fs.renameSync` (`page.cljs:201-227`, `electron/handler.cljs:149-151`). **Journals are not renamed.**
- **References across the graph are rewritten** (`rename-update-refs!` `:393-423`). For every block that
  refs the page, the block content is rewritten and each affected page file is re-serialised in full:
  - `[[Old Name]]` → `[[New Name]]`, a **case-sensitive** string replace of the original name
    (`replace-page-ref!` `:229-255`). Org file links are also handled when `:org-mode/insert-file-link?`
    is set.
  - `#Old` → `#New`, or `#[[New Name]]` if the new name contains whitespace. Case-insensitive, and only at
    word boundaries (`:257-270`).
  - Property keys `old::` → `new-name::` (lower-cased, spaces → `-`) (`:272-278`). Property values are
    walked the same way (`:289-311`).
- `config.edn` is updated: `:default-home :page` (`:487-489`) and `:favorites` (`:497-500`).

### 10.3 Delete

`page.cljs:352-383` (`delete!`) → `delete-file!` (`:180-189`) → `fs/unlink!` (`frontend/fs.cljs:77-81`) →
Electron `:unlink` (`electron/handler.cljs:51-66`):

- The file is **moved** to `logseq/.recycle/<relative-path with "/" and "\" replaced by "_">`. For example,
  `pages/foo.md` → `logseq/.recycle/pages_foo.md`, and an existing recycled file with the same name is
  overwritten by `renameSync`.
  - Exception: files under the dot-dir or plugin assets are unlinked for real.
  - The Capacitor/mobile implementation is the same (`fs/capacitor_fs.cljs:355-366`).
- If another page aliases the deleted page, the page entity is kept with its attributes stripped
  (`:369-376`). The page is also removed from `:favorites`.
- References elsewhere are **not** rewritten. `[[Deleted]]` remains as a dangling ref, which simply
  re-creates a file-less page.
- An external delete detected by the watcher (`watcher_handler.cljs:104-110`) removes the page from the
  DB without touching the disk.

---

## 11. Backups: `logseq/bak/` and `logseq/version-files/`

Implementation: `src/electron/electron/backup_file.cljs`.

- **Path**: `logseq/bak/<relative dir>/<file stem>/<ISO-8601 timestamp with ":" → "_">.Desktop.<ext>`.
  - Example: `logseq/bak/pages/foo/2025-11-14T09_30_12.345Z.Desktop.md`.
  - Mobile uses the same directories (`fs/capacitor_fs.cljs:153-176`).
- **Retention**: only the 6 newest files per directory are kept (`truncate-old-versioned-files!`, `:27-34`).
- **When a backup is written** (all to `:backup-dir`):
  1. Logseq writes a file whose on-disk content differs (after trim) from the DB's copy, and Google
     diff-match-patch shows that the write *deletes* something. The old disk content is backed up
     (`fs/node.cljs:52-60` → `electron/handler.cljs:69-80`).
  2. An external add/change arrives while the DB already had non-blank content for that file. The DB
     content is backed up before the disk version is accepted (`watcher_handler.cljs:44-56,86-89`).
  3. `writeFileSync` fails. The *new* content is saved to bak and the user is notified
     (`electron/handler.cljs:127-145`).
  4. A case-only rename conflict (`handler/common/file.cljs:38-40`).
- If on-disk content differs from the DB and the extension is not `excalidraw`/`edn`/`css`, Logseq
  **does not write**. It raises `:file/not-matched-from-disk` and asks the user to choose
  (`fs/node.cljs:44-50`).
- **`logseq/version-files/local/…`**: the same layout, filled by `:addVersionFile` for Logseq Sync. Remote
  versions are downloaded under `logseq/version-files/<…>` (`handler/file_sync.cljs:130-149`).
- Bitacora does not need to produce either folder, but **must ignore them** when indexing and must not
  delete them.

---

## 12. Metadata stored outside the graph folder

| Location | Content | Bitacora |
|---|---|---|
| `~/.logseq/config/config.edn` | Global config, merged under the graph config (`handler/global_config.cljs`) | SHOULD read (optional) and never write |
| `~/.logseq/graphs/<repo>.transit` | Serialized Datascript DB cache per graph. Name: `logseq_local_` + absolute path with `/`→`++` and `:`→`+3A+` (`electron/handler.cljs:206-224,284-…`). Rebuilt with "Re-index". | Ignore. Bitacora's own index lives elsewhere. |
| `~/.logseq/git/<path with "/"→"_", ":"→"comma">/.git` | Separate git dir when auto-commit is enabled. The graph then contains a `.git` **file** with `gitdir: …` (`electron/git.cljs:15-22,45-60,62-…`). | Must not delete or "fix" a `.git` file. See [[05-git-and-apis]]. |
| `~/.logseq/plugins/`, `~/.logseq/settings/`, `~/.logseq/preferences.json` | Plugins and their settings | Ignore |
| Electron `userData/configs.edn` | App-level settings (proxy, chmod, …) (`electron/configs.cljs:8-11`) | Ignore |
| Electron `userData/search/*.sqlite` | Full-text search index (`electron/search.cljs:125-145`) | Ignore |
| Browser IndexedDB / localStorage | NFS handles, UI state, recent pages, **asset alias dirs** (`:assets/alias-dirs`) | Ignore. Preserve `@alias/…` links verbatim. |
| `logseq/pages-metadata.edn` (in graph, legacy) | Old created/updated timestamps | Ignore, don't delete |

None of these files are needed to interpret a graph. Logseq can rebuild everything from the directory,
which is what "Re-index" does.

---

## Compatibility requirements for Bitacora

1. **MUST** treat the graph directory as the only source of truth and keep no sidecar files inside it. If
   Bitacora needs its own state in the graph, it MUST live under a dot-directory (e.g. `.bitacora/`),
   because Logseq ignores dot-paths (§1.1).
2. **MUST** implement Logseq's ignore rules when scanning:
   - skip dot-paths, symlinks, `node_modules/`, `.DS_Store`, `logseq/bak/`, `logseq/.recycle/`,
     `logseq/version-files/`, `logseq/graphs-txid.edn` and `logseq/pages-metadata.edn`;
   - apply `:hidden` as root-relative path prefixes.
3. **MUST** read `logseq/config.edn` as EDN, merged over the optional `~/.logseq/config/config.edn`. Maps
   are shallow-merged, the graph config wins, and a missing `:file/name-format` means **`:legacy`**.
4. **MUST** edit `config.edn` surgically (comment- and format-preserving, rewrite-edn style), changing only
   the keys it intends to change.
5. **MUST** derive a page title in this order: `pages/contents.*` → `Contents`; then the first-block
   `title::` (or front-matter `title:`); then the file body (after the last-dot split) decoded with the
   active `:file/name-format`.
6. **MUST** implement `tri-lb-file-name-sanity` / `tri-lb-title-parsing` bit-exactly, including:
   - the `%XX` pre-escape;
   - the reserved set ``: * ? " < > | # \``;
   - the leading-dot `%2E`;
   - Windows reserved names and trailing `.` → `___`;
   - underscore disambiguation (`___`, `_/`, `/_` → `%5F`);
   - per-token `%XX` decoding and empty-segment removal.

   The examples in §3.2 and `src/test/frontend/db/name_sanity_test.cljs` MUST be used as test vectors.
7. **MUST** implement legacy decoding (`.`→`/`, then a whole-string URL-decode). For legacy graphs, Bitacora
   **MUST** write `title::` whenever the title would not survive an encode→decode round-trip, as Logseq
   does.
8. **MUST** key pages by `lower-case + NFC` of the title, with one boundary `/` stripped. It MUST preserve
   the original case in file names and display. On duplicate titles it MUST keep the first file (Logseq's
   order) and report the conflict instead of merging.
9. **MUST** NFC-normalise paths and store them graph-relative with `/` separators.
10. **MUST** detect journals by parsing the title with
    `[page-title-format, "MMM do, yyyy", "yyyy-MM-dd", "yyyy_MM_dd"]` after capitalising each word. It MUST
    compute `journal-day = yyyyMMdd` and render the title with `:journal/page-title-format` (Java/Joda-style
    patterns such as `MMM do, yyyy`, including the ordinal `do`).
11. **MUST** name new journal files `<:journals-directory>/<format(:journal/file-name-format | "yyyy_MM_dd")>.md`.
    It MUST never rename existing journal files.
12. **MUST** create new pages at `<:pages-directory>/<encoded title>.md`. It MUST NOT create a file until
    the page has non-blank content. It MUST NOT create files for alias pages, namespace parents or merely
    referenced pages.
13. **MUST**, on rename: rename the file in place (same directory and extension); rewrite `title::` where
    present; rewrite `[[Old]]`, `#Old`/`#[[Old]]` and `old::` across all files; rename namespace children;
    and update `:favorites` / `:default-home` in `config.edn`.
14. **MUST** delete pages by moving the file to `logseq/.recycle/<rel path with / → _>`, never by
    unlinking. The same applies to deleted assets.
15. **MUST** save pasted files to `assets/<stem with space/%/ "/" → _>_<epoch-ms>_<index><ext>` (with `_`
    runs collapsed). It MUST link them with a path relative to the page file (`../assets/…`), using `![…]`
    for images, audio, video and PDF, and `[…]` otherwise.
16. **MUST** preserve byte-for-byte every file it does not understand or does not own: `.org`,
    `whiteboards/*.edn`, `draws/*.excalidraw`, `.tldr`, `custom.css`/`custom.js`/`export.css`, the
    `logseq/bak`, `.recycle` and `version-files` folders, and any `.git` file or directory.
17. **MUST** read and write UTF-8 and write LF line endings. It MUST NOT add a BOM. It SHOULD strip a
    leading BOM from the parsed text only, without rewriting the file unless the page is edited.
18. **MUST NOT** rewrite a file whose on-disk content (trimmed) differs from Bitacora's last-known content
    without first backing up or asking the user (Logseq's `not-matched-from-disk` semantics).
19. **SHOULD** write backups in Logseq's layout
    (`logseq/bak/<dir>/<stem>/<ISO ts with _>.<Client>.<ext>`, keeping the newest 6) before destructive
    overwrites, so Logseq users find them where they expect.
20. **SHOULD** keep YAML front matter as is when present, and SHOULD write new page properties as
    `key:: value` lines in the first, un-bulleted block, followed by a blank line.
21. **SHOULD** write `:file/name-format :triple-lowbar` (and embed Logseq's exact template config) when
    creating a new graph.
22. **SHOULD** index `.org` pages read-only (title and refs) so that references resolve. It SHOULD NOT
    create a `.md` twin for a page backed by `.org`.
23. **SHOULD** watch the graph directory and re-parse files changed externally (Logseq, git, sync), using
    trimmed-content comparison to drop no-op events.
24. **MAY** ignore everything under `~/.logseq` except the global `config.edn`, as well as `metadata.edn`,
    `pages-metadata.edn` and `graphs-txid.edn`.

## Open questions

1. **Initial journal file content**: when Logseq opens on a graph that already has content, does it write
   `journals/<today>.md` containing just `-` as soon as the auto-created empty block is saved, or only after
   typing? `create!` with `:create-first-block?` inserts an empty block, and `transform-content` serialises
   an empty block as `-`, which is not blank. This needs an empirical check, because it decides whether
   Bitacora should mimic or avoid empty journal files.
2. **Rename reference rewriting is case-sensitive** for `[[Old Name]]` (`string/replace` with the original
   name). Refs with different casing (`[[old name]]`) seem to be left untouched, while `#tag` replacement is
   case-insensitive. Should Bitacora copy this quirk exactly or fix it? Fixing it is safer for users and
   remains Logseq-readable.
3. **Custom `:journal/file-name-format`** values outside the parser's list (e.g. `yyyyMMdd`) appear
   unparseable as journals after a re-index. We need to confirm this with Logseq 0.10.x before deciding
   whether to support such graphs.
4. **`:pages-directory` vs `pages/contents.`**: the Contents special case and `create-contents-file` hard-code
   `pages/`. How should a graph with a custom `:pages-directory` treat its contents page?
5. **Whiteboard and draw page names** come from the EDN `:block/name` or a legacy-style decoded file name.
   Should Bitacora expose them as pages (so that `[[whiteboard]]` refs resolve) or only as attachments?
6. **CRLF files**: mldoc parses them, but do properties and block content keep `\r`? If they do, round-tripping
   an unedited block could change bytes. This needs a check against mldoc (see [[03-parsing-indexing-search]]).
7. **`title::` in triple-lowbar graphs**: Logseq never adds it automatically there, but it does honour and
   rewrite it. When Bitacora renames a page that has a `title::` but whose old name does not appear in the
   first block, Logseq leaves the stale `title::`. Should Bitacora always rewrite it?
8. **Multiple graphs sharing `~/.logseq/config/config.edn`**: should Bitacora honour the global config by
   default, or only on opt-in?
