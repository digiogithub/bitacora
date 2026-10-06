# Logseq Markdown Dialect and Block/Outline Syntax

> Analysis of how Logseq (file-based graph) reads and writes Markdown pages, for Bitacora's Rust parser/serializer.
> Source analysed: `/www/Bitacora/logseq` (commit `03bcefb`, 2025-11-14). All `path:LINE` references are relative to that repo unless prefixed `mldoc:`.
> Related: [[01-file-graph-layout]] (where files live, file names ↔ page names), [[03-parsing-indexing-search]] (what is indexed from the AST), [[04-editor-outliner-operations]] (which UI operations mutate the text).

## Summary

- **The snapshot is the file-graph code line.** It has no `db_based`/`file_based` handler split; there is a single code path, and it is the Markdown/Org file graph. The parser is **mldoc 1.5.7** (`package.json:130`), an OCaml/Angstrom parser compiled to JS. Logseq calls it through `parseJson`, `parseInlineJson`, `getReferences` and `astExportMarkdown` (`deps/graph-parser/src/logseq/graph_parser/mldoc.cljc:19-22`). `node_modules` was not installed in the repo. The AST examples below were produced by running `mldoc@1.5.7` from npm in a scratch directory. The OCaml grammar was read from `github.com/logseq/mldoc` HEAD; there is no v1.5.7 tag, so where HEAD and 1.5.7 differ this document reports the empirically observed 1.5.7 behaviour.
- **Outline model.** In Markdown, every line starting with optional indentation followed by `-` and whitespace is an mldoc `Heading` with `unordered: true` and `level = (number of indentation chars) + 1`. Every tab or space counts as 1. A top-level ATX heading line (`# x`) is also a block. Everything up to the next block line belongs to the current block's **body**: paragraphs, property drawers, code fences, and so on. Text before the first block is the **pre-block**, which holds page properties.
- **The block's stored content is a raw substring of the file.** Logseq slices the UTF-8 bytes between consecutive block start positions. It then strips the bullet (`- `) and the continuation indentation. The result is `:block/content`, which **includes** property lines, SCHEDULED/DEADLINE lines and `:LOGBOOK:` drawers. Properties are parsed out separately, but the text stays in the content.
- **Writing is a full-page re-serialization from the DB.** On any edit to a page, Logseq regenerates the whole file from the block tree (`src/main/frontend/modules/file/core.cljs:93-110`). Untouched blocks are re-emitted from their stored content. Because of this, Logseq *normalizes* the following:
  - indentation, which becomes tabs by default;
  - continuation-line indentation;
  - blank lines between blocks, which are dropped;
  - leading and trailing whitespace of each block;
  - CRLF line endings, which become LF;
  - Markdown `:PROPERTIES:` drawers, which become `key:: value` lines;
  - a first-block heading, which loses its `- `.

  It also adds `id::` and `collapsed::` lines when needed. Byte-exact round-trip of *untouched* files is therefore not something Logseq itself guarantees. Bitacora must (a) read everything Logseq reads and (b) write output that Logseq parses identically. It should do better than Logseq by not rewriting untouched blocks, so that git diffs stay small.
- **Properties use `key:: value`**, one per line. They are recognized anywhere in a block body, but by convention they sit right after the first line. A few built-in keys are UI/state metadata (`collapsed`, `id`, `card-*`, `query-*`, `hl-*`, …). The classification table is in §5.4.
- **Recommendation for Bitacora:** a custom, line-based outline splitter that keeps raw bytes per block, plus a custom inline scanner for Logseq-specific tokens (refs, tags, macros, properties, timestamps, markers). Use `pulldown-cmark` or `comrak` only to *render* the body of a block. Do not use them to segment the outline.

---

## 1. How Logseq invokes mldoc

| Function | Location | Purpose |
|---|---|---|
| `default-config` | `deps/graph-parser/src/logseq/graph_parser/mldoc.cljc:57-75` | Builds the JSON config. Defaults: `{:toc false :parse_outline_only false :heading_number false :keep_line_break true :format "Markdown" :heading_to_list false}`. |
| `->edn` | `mldoc.cljc:133-148` | `parseJson` → EDN, then `update-src-full-content` (adds `:full_content` to `Src` from the raw bytes, `mldoc.cljc:102-115`), then `collect-page-properties` (`mldoc.cljc:117-131`). The last step gathers **every** top-level `Directive` node (`#+key: v` or YAML front-matter `key: v`) anywhere in the file into one synthetic `["Properties" …]` node that is put first. |
| `inline->edn` | `mldoc.cljc:150-159` | Inline-only parse (`parseInlineJson`). Used for property values and links. |
| `get-references` | `mldoc.cljc:46-49` | Extracts refs from a property value string. |
| `remove-indentation-spaces` | `mldoc.cljc:77-100` | Strips the continuation-line indentation (see §2.4). |
| `extract` | `deps/graph-parser/src/logseq/graph_parser/extract.cljc:216-245` | File → pages and blocks. Page properties come from the first AST node if it is `Properties` or `Property_Drawer` (`extract.cljc:227-241`). |
| `extract-blocks` | `deps/graph-parser/src/logseq/graph_parser/block.cljs:646-693` | Walks the AST *in reverse*. Each `Heading` closes a block; `Property_Drawer`, SCHEDULED/DEADLINE paragraphs and the rest of the body attach to the preceding `Heading`. |
| `with-parent-and-left` | `block.cljs:695-768` | Turns flat levels into a parent/left-sibling tree. Tolerates inconsistent indentation. |
| `parse-block` / `parse-title-and-body` | `src/main/frontend/format/block.cljs:69-111` | Re-parses a single block after editing. Prefixes `"- "` before parsing (`:96-97`). |
| Malli AST schema | `deps/graph-parser/src/logseq/graph_parser/schema/mldoc.cljc:1-220` | Authoritative list of node shapes (§1.1). |

### 1.1 AST shapes (mldoc JSON → EDN)

The top level is a sequence of `[node, {start_pos, end_pos}]`. Positions are **UTF-8 byte offsets** (`block.cljs:657`, `utf8/encode` and `utf8/substring`).

Block nodes (`schema/mldoc.cljc:156-214`):
`Paragraph [inline…]`, `Paragraph_Sep n`, `Heading {title, tags, marker?, level, numbering?, priority?, anchor, meta, unordered, size?}`, `List [{content, items, number?, name, checkbox?, indent, ordered}]`, `Directive k v`, `Example [lines]`, `Src {lines, language?, options?, pos_meta}`, `Quote [blocks]`, `Export type options content`, `CommentBlock [lines]`, `Custom type options result content` (any `#+BEGIN_X`, including `query`), `Latex_Fragment`, `Latex_Environment`, `Displayed_Math s`, `Drawer name [lines]`, `Property_Drawer [[k v refs]…]`, `Footnote_Definition`, `Horizontal_Rule`, `Table {header, groups, col_groups}`, `Comment s`, `Raw_Html s`, `Hiccup s`, and the synthetic `Properties`.

Inline nodes (`schema/mldoc.cljc:64-126`):
`Emphasis [[Italic|Bold|Underline|Strike_through|Highlight] [inline…]]`, `Break_Line`, `Hard_Break_Line`, `Verbatim`, `Code`, `Tag [inline…]`, `Spaces`, `Plain`, `Link {url:[File|Search|Complex|Page_ref|Block_ref|Embed_data …], label, title?, full_text, metadata}`, `Nested_link {content, children}`, `Target`, `Subscript`, `Superscript`, `Footnote_Reference`, `Cookie`, `Latex_Fragment [Inline|Displayed s]`, `Macro {name, arguments}`, `Entity`, `Timestamp [Scheduled|Deadline|Date|Closed|Clock|Range …]`, `Radio_Target`, `Export_Snippet`, `Inline_Source_Block`, `Email`, `Inline_Hiccup`, `Inline_Html`.

Observed example (mldoc 1.5.7):

```text
- TODO [#A] task #tag
  SCHEDULED: <2024-01-01 Mon .+1d>
```
```json
[["Heading",{"title":[["Plain","task "],["Tag",[["Plain","tag"]]]],"marker":"TODO","priority":"A",
   "level":1,"unordered":true,"size":null, ...}],{"start_pos":0,"end_pos":22}],
[["Paragraph",[["Plain","  "],["Timestamp",["Scheduled",{"date":{"year":2024,"month":1,"day":1},
   "wday":"Mon","repetition":[["Dotted"],["Day"],1],"active":true}]],["Break_Line"]]], ...]
```

---

## 2. Outline structure

### 2.1 Grammar (EBNF-ish, Markdown file graph)

```ebnf
file          = [ bom ] [ front_matter ] [ pre_block ] { block } ;
front_matter  = "---" EOL { yaml_kv EOL } "---" EOL ;          (* only at byte 0; mldoc: lib/syntax/markdown_front_matter.ml *)
yaml_kv       = key ":" ws* value ;                            (* → Directive(key,value) → page property *)
pre_block     = { line } ;                                      (* everything before the first block line;
                                                                  usually page properties "k:: v" *)
block         = block_line { body_line } ;
block_line    = indent "-" ( ws | EOL | EOF ) [ ws* heading_hashes ] [ ws+ marker ] [ ws+ priority ] [ ws* title ]
              | indent heading_hashes ( ws | EOL ) title ;      (* ATX heading line without bullet *)
indent        = { " " | "\t" } ;                                (* level = len(indent) + 1, each char counts 1 *)
heading_hashes= "#" { "#" } ;                                   (* → Heading.size = count; must be followed by ws/EOL *)
marker        = "TODO" | "DOING" | "DONE" | "LATER" | "NOW" | "WAITING" | "WAIT"
              | "CANCELED" | "CANCELLED" | "STARTED" | "IN-PROGRESS" ;   (* must be followed by a space *)
priority      = "[#" any_char "]" ;                             (* usually A|B|C *)
title         = rest of line ;                                  (* inline-parsed *)
body_line     = any line that does not start a new block_line and is not inside a fenced/#+BEGIN construct
                that started in this block ;
```

Notes:

- `-` **must** be followed by whitespace or end-of-line. `-foo` is a paragraph (`mldoc:lib/syntax/heading0.ml:71-75`). `#foo` (no space) is a **tag**, not a heading.
- `*`, `+` and `1.` lines are **not** blocks. They are Markdown lists (`List`) inside the current block's body.
- A bullet line inside an open code fence or `#+BEGIN_…` region does **not** start a block. This holds even at column 0, for example `- x` inside a ```` ``` ```` fence (observed).
- `level` comes from raw indentation characters: `"  - b"` → 3, `"    - c"` → 5, `"\t- d"` → 2. Logseq builds the tree by *relative* comparison of these numbers (`block.cljs:695-768`). Mixed tabs and spaces, or "wrong" indentation, still produce a tree. A line indented further than its predecessor becomes a child, whatever the delta (`block.cljs:723-738`). An outdent goes to the nearest ancestor with indent ≤ current. If none matches exactly, the block becomes a sibling of the first deeper ancestor (`block.cljs:740-766`). Regression fixture: `deps/graph-parser/test/logseq/graph_parser/extract_test.cljs:81-87` (`"- line1\n    - line2\n      - line3\n     - line4"`).
- Whitespace for bullets, headings and property keys is ASCII space and tab only; a no-break space is not whitespace (`- a` is a paragraph). A lone `\r` is not a line break; `\r\n` is.
- ATX heading lines are recognised at **any indentation**, including inside a block body (`- a\n  ## h` starts a new block); the hashes may be any number and the line may be just `##`. mldoc reports the indent-based level, but Logseq forces it to 1 (`block.cljs:568-572`).
- ATX headings without a bullet (`## hello`) are blocks forced to level 1 (`block.cljs:568-572`). Following bullets indented under them become children (`extract_test.cljs:26-32`): `"## hello\n    - world"` gives `world` as a child of `## hello`.

### 2.2 Indentation written by Logseq

`:export/bullet-indentation` controls the indent unit **when writing files**, not only when exporting. It is used by `transform-content` at `src/main/frontend/modules/file/core.cljs:72-75` through `state/get-export-bullet-indentation` (`src/main/frontend/state.cljs:553-563`):

| config value | unit |
|---|---|
| `:tab` (default) | `"\t"` |
| `:two-spaces` | `"  "` |
| `:four-spaces` | `"    "` |
| `:eight-spaces` | `"        "` |

The config template documents it at `src/resources/templates/config.edn:108-114`.

### 2.3 Multi-line blocks (continuation lines)

The body of a block at tree depth `d` (1-based) is written with prefix `unit*(d-1) + "  "`. The two spaces align the text under the bullet text:

```markdown
- level 1 first line
  level 1 second line
	- level 2 first line
	  level 2 second line
```

The docstring at `mldoc.cljc:77-87` and `src/main/frontend/modules/file/core.cljs:15-18,72-75,81` describe this.

### 2.4 How block content is extracted on read

`get-block-content` (`block.cljs:406-422`) does the following:

1. Take the byte slice `[start_pos(this block), start_pos(next block))`. For the last block the slice runs to EOF (`block.cljs:665-670`).
2. `remove-level-spaces` (`deps/graph-parser/src/logseq/graph_parser/text.cljs:51-77`): left-trim, then strip `^[-]+\s?`. Content starting with `---` is left alone (front matter).
3. For non-pre-blocks: `remove-indentation-spaces(content, level+1, false)` (`mldoc.cljc:77-100`). For each continuation line, if the first `level+1` **characters** are all whitespace they are dropped and the rest is kept, including extra leading spaces. Otherwise the line is fully left-trimmed. `level` here is the mldoc raw level (indent chars + 1). With tabs this removes `(d-1)` tabs plus 2 spaces. With 2-space indentation it removes the right number of spaces.
4. Markdown only: `->new-properties` (`deps/graph-parser/src/logseq/graph_parser/property.cljs:135-158`) rewrites an org-style `:PROPERTIES: … :END:` drawer inside a Markdown block into `key:: value` lines. `id`/`custom_id`/`custom-id` become `id`, `last-modified-at` becomes `updated-at`, and `_` in keys becomes `-`. **This is a content-changing normalization on read.** It is persisted on the next write.

Fixtures: `src/test/frontend/fs/diff_merge_test.cljs:58-80`, `deps/graph-parser/test/logseq/graph_parser/mldoc_test.cljs:122-137`.

### 2.5 Pre-block (page properties)

- Everything before the first block line becomes a block with `:block/pre-block? true` (`block.cljs:515-553`). It covers all bytes `[0, first_block_start)`. If the file has **no** block lines at all, the whole file becomes one pre-block. Its content is the raw text, not de-indented.
- If the pre-block's properties contain `heading`, it is *not* treated as a pre-block (`block.cljs:532`).
- Page properties = properties of the first AST node when it is `Properties` (front matter / `#+key:` directives) or `Property_Drawer` (`k:: v` lines) (`extract.cljc:227-241`).
- `#+key: value` lines anywhere in the file become `Directive` nodes. `collect-page-properties` hoists **all** of them into the page properties (`mldoc.cljc:117-131`), so `- #+title: x` deep in the page still sets the page title. Bitacora should replicate this, or at least flag it.
- YAML front matter (`---\nkey: value\n---`) is parsed only at byte 0. It yields page properties and is preserved verbatim, because the pre-block content is raw.

Example:

```markdown
title:: My Page
alias:: Mine, [[My page alias]]
tags:: project, [[multi word]]

- first block
```

AST: `Property_Drawer [["title","My Page",[]], ["alias", …], ["tags","project, [[multi word]]",[Link Page_ref "multi word"]]]`, then `Paragraph [Break_Line]`, then `Heading`.

### 2.6 Headings as blocks

- `- # Title` → Heading with `size: 1`. Logseq stores `:block/properties {:heading 1}` (`block.cljs:555-583`), but **does not write** a `heading::` property for numeric sizes. The `#`s live in the content. `wrap-parse-block` drops a numeric `:heading` (`src/main/frontend/handler/editor.cljs:304-307`).
- `heading:: true` is an explicit property meaning "auto heading by depth". It is written to the file (`editor.cljs:3822-3862`, `set-heading-aux!`).
- Changing a heading level rewrites the `#` prefix (`src/main/frontend/commands.cljs:624-638`).
- **Top heading without bullet:** the first top-level block of a page that has a `heading` property is written **without** `- ` (`core.cljs:52-55,63-64,82-85`). `- # Title` as the first block is rewritten as `# Title`. ATX headings that are not first are written as `- ## x`.

### 2.7 Empty lines and empty blocks

- Blank lines between blocks are part of the previous block's slice. They are removed by `string/trim` on write (`core.cljs:81`). **Logseq never writes blank lines between blocks**, except the single blank line after a pre-block: the pre-block is written as `trim(content) + "\n"` and then joined with `"\n"` (`core.cljs:45-48,108-110`).
- Blank lines *inside* a multi-line block are kept. On write they receive the continuation prefix, so an empty line inside a level-1 block becomes a line with two spaces `"  "` (`core.cljs:15-18`).
- An empty block is written as a bare `-` with no trailing space (`core.cljs:82-86`). `- ` with a trailing space is parsed as an empty Heading. A trailing `" "` alone after it becomes a paragraph (observed).
- The file is the blocks joined with `"\n"`, so there is **no trailing newline at EOF** (`core.cljs:108-110`) unless the page is pre-block-only.

---

## 3. Properties

### 3.1 Syntax (mldoc `lib/syntax/markdown_property.ml`)

```ebnf
property_line = ws* key "::" ( " " ws* value | ws* EOL ) ;
key           = 1*( any char except ":" , whitespace, EOL ) ;     (* "my key:: v" is NOT a property; "a.b.c::" ok *)
value         = rest of line, trimmed ;
```

- `key::value` with no space after `::` is **not** a property; it stays a paragraph. `key::` alone at end of line is a property with an empty value (observed; mldoc tests `test_markdown.ml:750`).
- Observed on mldoc 1.5.7 (`bitacora-markdown` `properties/scan.rs`): text right after the bullet can be a property line (`- a:: b` is an empty-title heading plus a drawer); after `::` the next byte must be a space (`a::\tb` is text) or only whitespace follows; keys never contain `:` (`a:b:: c`, `a::: b` are text); a blank line or any text line ends a property group, so a block can hold several groups; a `#+name: v` line joins only when it *follows* a property line.
- One or more consecutive property lines form one `Property_Drawer`. `#+name: value` lines adjacent to them merge into the same drawer (mldoc `drawer.ml`, test `"a:: 1\n#+b: 2"`).
- Recognized in any body position, including mid-block after text (observed: `- a\n  text\n  late:: prop` yields a `Property_Drawer`). `extract-blocks` attaches **any** `Property_Drawer` in the body to the block. If there are several, the *first* one wins because the AST is iterated in reverse (`block.cljs:677-679`). The UI functions assume properties directly follow the first (title) line, or start the block (`src/main/frontend/util/property.cljs:129-167, 226-316`).
- Not recognized inside quotes (`> a:: b`), code fences or `#+BEGIN` blocks.
- Org-style `:PROPERTIES:\n:key: value\n:END:` also parses in Markdown, and is converted on read (§2.4).
- Key normalization on extract (`block.cljs:204-238`): lowercase, `" "` and `_` become `-`, `custom_id`/`custom-id` become `id`. Keys must be valid EDN keywords without `"^(){}` and must not start with `#` (`property.cljs:22-28`). Invalid keys go into `:block/invalid-properties` and are ignored, but stay in the text.
- Order is kept in `:block/properties-order`. The original text of each value is kept in `:block/properties-text-values`.

### 3.2 Value parsing (`text.cljs:87-187`, mldoc `lib/syntax/property.ml`)

The following rules apply in order:

1. Keys in `unparsed-built-in-properties` (`property.cljs:110-121`), or in config `:ignored-page-references-keywords`, keep the raw trimmed string.
2. Values wrapped in `"…"` are kept as raw strings, quotes included. `tags:: "foo, bar"` gives the string `"\"foo, bar\""`.
3. Otherwise, refs in the value (`[[page]]`, `#tag`, `#[[multi word]]`, `[[nested [[x]]]]`) give a **set of page names**. Macros are skipped (`inline_skip_macro`).
4. Keys in `#{:alias :aliases :tags}`, plus config `:property/separated-by-commas`, are also split on `,` or the full-width `，`, and plain fragments become page names: `tags:: foo, bar` gives `#{"foo" "bar"}` (`text.cljs:132-163`). Other keys are **not** split: `foo:: a, b` stays the string `"a, b"`.
5. `true`/`false` become booleans, and `^\d+$` becomes an integer (`text.cljs:87-98`). `"\"1000\""` stays a string.
6. Built-in typed keys (`property.cljs:81-103`): `collapsed`, `heading`, `public`, `query-table` … are booleans, and `created-at`, `updated-at`, `hl-page`, `hl-stamp`, `todo`/`doing`/`now`/`later`/`done` are integers.

Tests: `deps/graph-parser/test/logseq/graph_parser/block_test.cljs:45-111`, `text_test.cljs:64-115`, `mldoc_test.cljs:79-88`.

Property *names* also create pages ("property pages") unless `:property-pages/enabled? false` or the key is in `:property-pages/excludelist` or is built-in (`block.cljs:134-150`). That is relevant to indexing; see [[03-parsing-indexing-search]].

### 3.3 Writing properties (`src/main/frontend/util/property.cljs`)

- `insert-property` (`:226-316`): format is `key:: value`, with the value trimmed and the key lowercased (`:251-252`). If the key exists in the property group, its value is replaced in place. Otherwise the property is appended to the end of the existing property group. If the block has no properties, it is inserted after the first line when that line is a "title" (Paragraph/Heading/Raw_Html/Hiccup, `src/main/frontend/format/mldoc.cljs:36-41`), otherwise at the top. Tests: `src/test/frontend/util/property_test.cljs:89-167`.
- `insert-properties` with collection values writes `[[a]], [[b]]` (`:318-334`).
- `remove-property` removes the first line starting with `key:: ` (`:336-351`).
- **Editing hoists properties.** When a block with properties is saved after a text edit, `with-built-in-properties` (`:180-219`, called from `editor.cljs:308-310`) collects **all** `k:: v` lines in the block. It re-emits them directly after the title, hidden built-ins first and then the user's lines, followed by the rest of the body. Properties scattered in the body are therefore moved up when Logseq edits a block.
- On save, hidden properties from the DB are merged back (`editor.cljs:345-352`). The editor textarea never shows hidden built-ins: they are removed by `remove-built-in-properties` (`property.cljs:361-377`) and re-added on save.

---

## 4. Block identifiers

- Syntax: `id:: 6500c1a4-0000-4000-8000-000000000001`, a normal property. It is parsed with `parse-uuid`. If it is missing or invalid, the block gets a **random, non-persisted** UUID (`block.cljs:424-432`).
- **When Logseq writes `id::`:**
  - copy block ref, copy embed, or any API that refs a block → `set-blocks-id!` (`editor.cljs:955-972`), which inserts `id::` into the target block's content;
  - safety net at serialization: if a block is referenced (`:block/_refs`) but its content does not contain its UUID string, `transform-content` inserts `id::` (`core.cljs:36-37,87-89`);
  - pre-blocks never get an `id` from `set-blocks-id!` (`editor.cljs:962`).
- Blocks that are never referenced have no `id::` in the file. Their identity across reloads relies on a 2-way **diff-merge** of block bodies when the file changes on disk (`src/main/frontend/handler/common/file.cljs:57-93`, `frontend.fs.diff-merge`; tests `src/test/frontend/fs/diff_merge_test.cljs`).
- Duplicate ids: if an id already exists in another page or earlier in the same file, the block gets a new UUID and the `id::` line is **removed from its content** (`block.cljs:610-644`). The file changes on the next write.
- A block cannot reference itself: `((own-uuid))` is stripped from its own content on save (`editor.cljs:324`).

---

## 5. References, tags, embeds

### 5.1 Syntax and AST

| Text | AST | Ref semantics |
|---|---|---|
| `[[page]]` | `Link{url:["Page_ref","page"], label:[Plain ""], full_text}` | page ref |
| `[[a [[b]] c]]` | `Nested_link{content:"[[a [[b]] c]]", children…}` | page named `a [[b]] c`; also `b` |
| `#tag` | `Tag [Plain "tag"]` | page ref + `:block/tags`; the tag ends at whitespace or `, ; . ! ? ' " :` (trailing punctuation is excluded, mldoc `extended/hash_tag.ml:5-30`) |
| `#[[multi word]]` | `Tag [Link Page_ref "multi word"]` | page ref |
| `((uuid))` | `Link{url:["Block_ref","uuid"], label:[]}` | block ref; the UUID must match `[0-9a-f]{8}-…-[0-9a-f]{12}` (`deps/graph-parser/src/logseq/graph_parser/util/block_ref.cljs:9`) |
| `[label]([[page]])` | `Link{url:["Page_ref","page"], label:[Plain "label"]}` | page ref with alias text |
| `[label](((uuid)))` | `Link{url:["Block_ref","uuid"], label:[Plain "label"]}` | block ref with alias text |
| `{{embed [[page]]}}` | `Macro{name:"embed", arguments:["[[page]]"]}` | page ref (`block.cljs:70-75`) |
| `{{embed ((uuid))}}` | `Macro{name:"embed", arguments:["((uuid))"]}` | block ref (`block.cljs:99-105`) |
| `[x](file:../pages/foo.md)` | `Link{url:["File",…]}` | page ref by label (`block.cljs:62-64`) |
| `[[draws/x.excalidraw]]`, `[[assets/x.pdf]]` | `Page_ref` but treated as a link, not a page (`mldoc.cljc:161-175`) | none |
| `` `[[x]]` `` | `Code "[[x]]"` | **no ref** |
| `\[[x]]` | `Plain "\\[[x]]"` (backslash kept in text) | no ref |

The ref walk is `with-page-refs` (`block.cljs:337-372`). It skips `Custom "query"` blocks, adds marker and priority as refs (`TODO`, `A`), and adds namespace parents for `a/b/c`. Ref extraction from properties is covered in §3.2. Exact pipeline: [[03-parsing-indexing-search]].

### 5.2 Macros and queries

- Grammar: `{{name args}}` or `{{{name args}}}`. `name` runs until a space, `(` or `}`. Arguments are comma-separated. An argument can be a `[[…]]` page ref, a `((…))` ref, a `"quoted string"` (commas allowed inside), or plain text up to `,` (mldoc `lib/syntax/inline.ml:1047-1090`). Macros cannot span lines.
- Built-in macro names rendered by Logseq (`src/main/frontend/components/block.cljs:1475-1540`): `query`, `function`, `namespace`, `youtube`, `youtube-timestamp`, `zotero-imported-file`, `zotero-linked-file`, `vimeo`, `bilibili`, `video`, `tweet`/`twitter`, `embed`, `renderer` (plugins). Registered ones include `cloze`, `cards` (`src/main/frontend/extensions/srs.cljs:60-66,760`) and `img`. User macros come from config `:macros {"name" "text with $1 $2"}` (`src/resources/templates/config.edn:261-268`). They are substituted at render time only (`src/main/frontend/format/block.cljs:113-123`) and **never written expanded** to the file.
- Advanced query: a `#+BEGIN_QUERY … #+END_QUERY` block parses to `Custom "query"`, with the raw EDN in `content`. Simple query: `{{query (and [[a]] (task TODO))}}`.
- Query view state is stored as hidden properties on the block that contains the query: `query-table:: true`, `query-properties:: [:page :block]`, `query-sort-by:: block`, `query-sort-desc:: false` (`editor.cljs:908-922`).

### 5.3 Tasks, scheduling, logbook

- Markers (mldoc `heading0.ml:16-28`): `TODO DOING DONE LATER NOW WAITING WAIT CANCELED CANCELLED STARTED IN-PROGRESS`. A marker is recognized only immediately after the bullet, or after the heading hashes, and **followed by a space**. In 1.5.7, `- LATER` with no trailing text is *not* a marker (observed), and `TODOx` is not one either. Logseq's regex is `src/main/frontend/util/marker.cljs:6-12` (`STARTED` is missing there). The marker cycles TODO→DOING→DONE→none and LATER→NOW→DONE (`marker.cljs:40-58`).
- Priority is `[#A]` right after the marker or bullet (`[#` + any single char + `]`).
- `SCHEDULED: <2024-01-01 Mon>` and `DEADLINE: <…>` on their own lines in the body. mldoc parses them as a `Paragraph` whose first or second inline is a `Timestamp`. Logseq lifts them to `:block/scheduled` and `:block/deadline` as `yyyyMMdd` ints, plus `:block/repeated?` (`block.cljs:240-271`).
  - Timestamp syntax: `<YYYY-MM-DD Www[ HH:MM][ repeater]>` is active and `[…]` is inactive. The repeater is `+Nu` (Plus), `++Nu` (DoublePlus) or `.+Nu` (Dotted), with `u` ∈ `h d w m y` (mldoc `inline.ml:1092-1150`).
  - Lines are written as `SCHEDULED: <…>`, uppercase key, after the title (`src/main/frontend/util/text.cljs:35-59`).
  - When a repeated task is marked DONE, Logseq advances the dates and appends a logbook entry `* State "DONE" from "TODO" [2024-01-01 Mon 10:00]` (`editor.cljs:676-706`).
- Drawers have the form `:NAME:` … `:END:` on their own lines, case-insensitive end marker → `Drawer name [lines]` (mldoc `drawer.ml:84-103`). `:LOGBOOK:` holds time tracking:
  - The clock-in line is `CLOCK: [2024-01-01 Mon 10:00:00]` (`src/main/frontend/util/clock.cljs:69-73`).
  - The clock-out line is `CLOCK: [start]--[end] =>  HH:MM:SS`, with **two spaces** after `=>` (`clock.cljs:75-93`).
  - Clock entries are written when the marker changes to DOING or NOW, and closed on DONE or a revert (`editor.cljs:256-296`). Disable with `:feature/enable-timetracking? false`.
  - The drawer is inserted after the title, SCHEDULED/DEADLINE lines and properties (`src/main/frontend/util/drawer.cljs:31-86`).
  - On save, an existing logbook is merged into the new content (`drawer.cljs:126-140`).

### 5.4 Built-in properties: content vs metadata

Sources: `deps/graph-parser/src/logseq/graph_parser/property.cljs:46-103`, `src/main/frontend/util/property.cljs:419-432`, `src/main/frontend/extensions/srs.cljs:48-53,763-768`, `src/main/frontend/extensions/pdf/assets.cljs:131-201`, `editor.cljs:74-96`.

"Hidden" means it is not shown in the UI property list. For the merge strategy, **Content** is user intent and should merge like text. **Metadata** is UI or app state where last-writer-wins or union is acceptable. **Identity** must never be lost or duplicated.

| Property | Scope | Hidden | Value type | Class | Notes |
|---|---|---|---|---|---|
| `id` (also `custom-id`/`custom_id`) | block | yes | UUID string | **Identity** | Written only when referenced or embedded or copied as ref. Never drop it. A conflicting duplicate gets regenerated. |
| `title` | page | yes (pre-block) | string | Content | Overrides the file-derived page name. |
| `alias` / `aliases` | page | no | set of pages (comma-split) | Content | |
| `tags` | page (and block) | no | set of pages (comma-split) | Content | |
| `template`, `template-including-parent` | block | no | string / bool | Content | |
| `public` | page | no | bool | Content (publishing setting) | |
| `icon` | page | yes (pre-block) | string | Content (presentation) | |
| `filters` | page | yes (pre-block) | EDN map `{"page" true}` | **Metadata** | Linked-references filter UI state. |
| `exclude-from-graph-view` | page | no | bool | Metadata-ish (view setting) | |
| `heading` | block | yes | `true` or int | Content (formatting) | Only `true` is written; numeric levels live in `#`s. |
| `collapsed` | block | yes | bool | **Metadata** (UI state) | Written as `collapsed:: true` when collapsed and removed when expanded (`core.cljs:20-32`). Very noisy in git. |
| `background-color` / `background_color` | block | yes | string (`red`, `#533e7d`…) | Content (presentation) | |
| `logseq.order-list-type` | block | yes | `number` | Content (presentation) | Numbered-list rendering. |
| `logseq.color`, `logseq.table.*` (`version`, `compact`, `headers`, `hover`, `borders`, `stripes`, `max-width`) | block | yes (block) | string/bool | Content (presentation) | |
| `logseq.query/nlp-date` | block | yes | bool | Metadata (query option) | |
| `logseq.macro-name`, `logseq.macro-arguments` | DB only | yes | — | Not in files | |
| `logseq.tldraw.page`, `logseq.tldraw.shape` | whiteboard blocks | yes | EDN | Metadata | Whiteboards are `.edn`, out of scope. |
| `created-at`, `updated-at`, `created_at`, `last-modified-at`, `last_modified_at` | block | yes | int ms | **Metadata** | Read from legacy files into `:block/created-at`. The current file graph keeps timestamps **in the DB only** (`src/main/frontend/modules/outliner/core.cljs:57-64,146`); it does not write them. |
| `query-table`, `query-properties`, `query-sort-by`, `query-sort-desc` | block | yes | bool / EDN vec / string / bool | **Metadata** (view state) | |
| `card-last-interval`, `card-repeats`, `card-last-reviewed`, `card-next-schedule`, `card-ease-factor`, `card-last-score` | block (with `#card`) | yes | numbers / ISO date strings | **Metadata** (SRS review state) | High-churn. Last-writer-wins is reasonable. |
| `ls-type` (`annotation`), `hl-type` (`area`), `hl-page`, `hl-stamp`, `hl-color` | block (PDF highlight pages `hls__*`) | yes | string / int | Metadata (tied to `.edn` highlight file) | |
| `todo`, `doing`, `now`, `later`, `done` | block | yes | int ms | **Metadata** (legacy marker timestamps) | |
| `macro`, `filetags` | org only | — | — | n/a for Markdown | |
| user-defined `foo:: …` | page/block | no (unless `:block-hidden-properties`) | per §3.2 | Content | |
| `:LOGBOOK:` drawer (not a property) | block | rendered collapsed | CLOCK lines | **Metadata** (time tracking) | Union-merge lines. |
| `SCHEDULED:` / `DEADLINE:` (not properties) | block | no | timestamp | Content | |

Config `:block-hidden-properties #{…}` (`config.edn:315-317`, `src/main/frontend/config.cljs:495`) adds user-hidden keys. Those keys are still content.

---

## 6. Inline and block formatting (Markdown mode)

Inline dispatch is `mldoc:lib/syntax/inline.ml:1408-1450` (keyed on the first char). The AST was observed with mldoc 1.5.7.

| Syntax | AST |
|---|---|
| `**b**`, `__b__` | `Emphasis Bold` |
| `*i*`, `_i_` | `Emphasis Italic` (`_` requires word-boundary flanking) |
| `***bi***` | nested Bold+Italic |
| `~~s~~` | `Emphasis Strike_through` |
| `^^h^^`, `==h==` | `Emphasis Highlight` (both) |
| `` `code` ``, ``` ``co`de`` ``` | `Code` (it beats emphasis and links: `*aa`*`` → `Plain "*aa"`, `Code "*"`) |
| `$x$` | `Latex_Fragment Inline` |
| `$$x$$` inline / `$$…$$` block | `Latex_Fragment Displayed` / `Displayed_Math` |
| `\(..\)`, `\[..\]` | Latex fragments |
| `[^1]` / `[^1]: def` | `Footnote_Reference` / `Footnote_Definition` |
| `a_{b}`, `a^{b}` | Subscript / Superscript |
| `![alt](../assets/a.png){:height 100, :width 200}` | `Link{url:Search, metadata:"{:height 100, :width 200}"}`. Logseq rewrites the `{…}` EDN map when the image is resized (`editor.cljs:1836-1844`, `pr-str`). |
| `[text](https://x)`, `<https://x>`, bare `https://x` | `Link Complex` |
| `<span>…</span>` | `Inline_Html` |
| `[:div "hi"]` | `Inline_Hiccup` (block-level `Hiccup` when on its own line) |
| `<2024-01-01 Mon>` / `[2024-01-01 Mon 10:00]` | `Timestamp Date` active / inactive |
| `<!-- … -->` on own lines, `[//]: # comment` | `Raw_Html` (1.5.7 observed) / `Comment` |
| ```` ```lang … ``` ````, `~~~` | `Src{language, lines}` (indentation stripped by `update-src-full-content`) |
| `> quote` | `Quote` (property lines inside are plain text) |
| `#+BEGIN_QUOTE/SRC/EXAMPLE/EXPORT/COMMENT/NOTE/TIP/IMPORTANT/CAUTION/PINNED/WARNING/CENTER/QUERY … #+END_X` | `Quote`/`Src`/`Example`/`Export`/`CommentBlock`/`Custom name` (case-insensitive, common indent stripped, mldoc `block0.ml:127-221`) |
| GFM table `\| a \| b \|` + `\|---\|` | `Table` |
| `---`, `***`, `___` on a line (not at file start) | `Horizontal_Rule` |
| `term\n: definition` | `List` with `name` (definition list, `mldoc_test.cljs:90-99`) |
| `* item`, `+ item`, `1. item` | `List` (not outline blocks) |
| trailing two spaces + newline | `Hard_Break_Line` |
| newline inside paragraph | `Break_Line` (`keep_line_break: true`) |
| `\` + punctuation | **not unescaped**; the backslash stays in `Plain` |

---

## 7. Serialization: how Logseq writes a page

The pipeline:

```
edit → outliner tx → frontend.modules.outliner.file/sync-to-file (debounced 1 s, outliner/file.cljs:17,86-117)
     → do-write-file! (whole page; long pages >500 blocks wait for idle, :46-70)
     → frontend.modules.file.core/save-tree! → tree->file-content → transform-content per block
     → file-handler/alter-files-handler! (writes, then re-parses the file)
```

`transform-content` (`src/main/frontend/modules/file/core.cljs:34-90`) works as follows, for block `b` at tree depth `level`:

```
content := b.content or ""
pre?    := b.pre-block?  OR (b is first top-level block AND markdown AND first line contains ":: ")
if pre?:
    out := trim(content) + "\n"                                 ; no bullet, no indentation
else:
    top_heading? := markdown AND b is first top-level block AND b.properties.heading
    unit   := config :export/bullet-indentation (default "\t")
    prefix := top_heading? ? "" : unit*(level-1) + "-"
    cont   := top_heading? ? "" : unit*(level-1) + "  "
    content := collapsed? ? insert-property(content, "collapsed", true)
             : collapsed? == false ? remove-property(content, "collapsed") : content
    body := join(split-lines(trim(content)), "\n" + cont)
    sep  := (top_heading? OR blank(body)) ? "" : " "
    out  := prefix + sep + body
if b is referenced AND content lacks str(b.uuid):
    out := insert-property(out, "id", b.uuid)                   ; note: applied after indentation
file := join(depth-first(out for each block), "\n")             ; no trailing newline
```

Consequences:

1. **Every write rewrites every block of the page.** Untouched blocks are re-emitted from `:block/content`. That loses the original indentation unit, inter-block blank lines, trailing whitespace on the first and last lines, and CRLF (`split-lines` splits on `\r?\n`). Byte-identical output happens only when the file already follows Logseq's canonical form.
2. A first top-level block whose first line contains `:: ` is written as a bullet-less pre-block. `- title:: x` as the first block becomes `title:: x` and turns into page properties on the next parse.
3. A first block with `heading:: true` (not numeric) is written without a bullet, as `text\nheading:: true`. On re-parse that becomes a pre-block paragraph. This is a Logseq quirk; Bitacora should not reproduce it.
4. Pre-block content keeps its internal bytes, including CRLF and front matter, and is only trimmed at the edges.
5. Multi-line content lines that were under-indented on read are left-trimmed (§2.4) and re-indented with the canonical prefix on write. For example, the tutorial's column-0 `#+BEGIN_TIP` lines under a bullet (`src/resources/tutorials/tutorial-en.md:5-10`) are re-indented.
6. If the page has a single blank block and no file, nothing is written (`outliner/file.cljs:63-65`). If the page is empty and not just deleted, an error is raised and nothing is written (`core.cljs:158-161`).

Canonical form, which is what Bitacora should emit:

```markdown
title:: Example
tags:: demo

- TODO [#A] Parent block #tag
  SCHEDULED: <2024-01-01 Mon .+1d>
  id:: 6500c1a4-0000-4000-8000-000000000001
  :LOGBOOK:
  CLOCK: [2024-01-01 Mon 10:00:00]--[2024-01-01 Mon 11:00:00] =>  01:00:00
  :END:
	- child with ((6500c1a4-0000-4000-8000-000000000001))
	  second line of child
	  ```clojure
	  (+ 1 2)
	  ```
	-
- collapsed parent
  collapsed:: true
	- hidden child
```

Note the order inside a block that Logseq produces when it edits: title, then SCHEDULED/DEADLINE, then properties, then the `:LOGBOOK:` drawer, then the rest of the body (`drawer.cljs:53-68`, `property.cljs:244-260`). Files written by hand may use other orders, and Logseq reads them fine.

---

## 8. Edge cases

| Case | Behaviour |
|---|---|
| `[[` inside inline code or a fence | No ref (`Code` / `Src`). |
| Property-like lines inside a fence or `#+BEGIN` | Not properties (observed `Src.lines`). But Logseq's `remove-property` and `insert-property` work on lines and are not code-aware (`property.cljs:336-351`). `remove-built-in-properties` skips blocks whose whole content is a fence (`:361-371`). |
| `- ` inside a fence at column 0 | Not a block; the fence wins. |
| Unclosed fence | mldoc falls back to paragraph parsing. **Open question:** does a following bullet still start a block? This needs fixtures. |
| CRLF | Parsed fine. mldoc treats `\r\n` as EOL (tests in `mldoc:test/test_markdown.ml` "endwith-carriage-return"). Block content keeps `\r` until `split-lines` on write. Writes produce LF, except inside the pre-block. |
| Tabs vs spaces mixed | Tree by relative raw indentation; output normalized to the configured unit. |
| UTF-8 / BOM | Positions are byte offsets. **Open question:** BOM handling (likely part of the first line). |
| Trailing whitespace | Lost at block edges; kept on inner lines; whitespace-only lines inside blocks get the continuation prefix. |
| Escaping | `\[[x]]` is not a ref, and the backslash is kept in the text and in rendering. `"quoted"` property values suppress ref parsing. |
| `#` + punctuation | `#tag.` → tag `tag`; `#foo:` → `foo`; `#[[a b]]` → `a b`. `tag-valid?` rejects names containing `#`, spaces or newlines (`deps/graph-parser/src/logseq/graph_parser/util.cljs:61-64`). |
| `- #+title: x` / `#+key: v` anywhere | Hoisted into page properties (`mldoc.cljc:117-131`). |
| Markdown `:PROPERTIES:` drawer in a block | Converted to `k:: v` on read and persisted (`property.cljs:135-158`). |
| Duplicate `id::` | The second block gets a new uuid, and its `id::` line is removed. |
| Invalid property key (`"x"::`, `(a)::`) | Kept in the text and listed in `:block/invalid-properties`. |
| Empty bullet `-` vs `- ` | Both are empty blocks; written as `-`. |
| `*`/`+`/`1.` lists at column 0 | Body of the current block (or of the pre-block). They are not outline nodes. |

---

## 9. Rust parser options

| Crate | Strengths | Problems for Logseq files |
|---|---|---|
| **pulldown-cmark** | Very fast pull parser. CommonMark plus GFM tables, strikethrough, tasklists, footnotes, math (0.10+), heading attributes. Gives source offsets (`into_offset_iter`). | Treats `- ` as a CommonMark list: lazy continuation, 4-space indented code, and loose/tight rules all differ from Logseq. No custom inline syntax (`[[ ]]`, `(( ))`, `{{ }}`, `#tag`, `key::`, `^^ ^^`, `==`). No lossless tree. |
| **comrak** | Full GFM AST with sourcepos, extensions (wikilinks `[[ ]]`, math, front matter, footnotes, description lists), and a CommonMark formatter. | Its formatter normalizes (not lossless). The list semantics are again CommonMark. Wikilink support does not cover nesting or `#[[ ]]`, and there are no block refs or property lines. |
| **markdown-rs** | micromark-faithful, mdast, MDX, GFM, frontmatter, math, precise positions. | Its extension API is limited (constructs are not pluggable from outside). Same list-semantics mismatch. |
| **tree-sitter-markdown** (split block/inline grammars) | Incremental concrete syntax tree with byte ranges. Good for editor highlighting. Grammars can be forked. | CommonMark-oriented. Extending it means maintaining a C grammar fork. Error recovery is ad hoc compared with mldoc's rules. |
| **Custom (recommended)** | Matches mldoc's line-dispatch design exactly (mldoc itself is a line-oriented dispatcher, `mldoc:lib/mldoc_parser.ml:188-275`). Lossless by construction. | More code. Needs a large fixture corpus. |

**Recommendation:**

1. **Layer 1, the outline splitter (custom, line-based, lossless).** Scan lines. Track open fences (```` ``` ````/`~~~`) and `#+BEGIN_X…#+END_X` regions so their lines never start blocks. Detect block lines (`indent '-' (ws|EOL)` and top-level ATX `#{1,6} `), front matter at byte 0, and the pre-block. Each block keeps its **raw byte range**, raw indentation string, bullet line and body lines. Build the tree with Logseq's relative-indent algorithm (`block.cljs:695-768`).
2. **Layer 2, the block-head and property scanner (custom).** Recognize marker, priority, heading size, `key:: value` runs (with mldoc's exact key rules), `SCHEDULED:`/`DEADLINE:` lines, and `:NAME:`…`:END:` drawers. Keep each element's byte span so that edits rewrite only those lines.
3. **Layer 3, the inline scanner (custom, small).** Handle refs (`[[…]]` with nesting, `((uuid))`, labelled refs), tags (with mldoc's delimiter set), macros, timestamps, and inline code and math spans, which suppress refs. This is used for indexing ([[03-parsing-indexing-search]]).
4. **Rendering:** pass the de-indented block body to `comrak` (or `pulldown-cmark`), after protecting Logseq tokens or using comrak's wikilink extension and post-processing them. Handle `^^`/`==` highlight, macros, hiccup and `#+BEGIN_*` blocks before Markdown rendering. Rendering never feeds back into the saved text.
5. **Writer:** for an edited block, emit Logseq's canonical form (§7). For untouched blocks, **copy the original bytes verbatim**. Re-serializing the whole page, as Logseq does, is unnecessary and causes merge conflicts.

---

## 10. Compatibility requirements for Bitacora

1. **MUST** treat `^[ \t]*-([ \t]|$)` lines outside fences and `#+BEGIN` regions as block starts. Level = indentation char count + 1. Build the tree by relative comparison using Logseq's algorithm, including the tolerant outdent rule.
2. **MUST** treat top-level `#{1,}[ \t]` lines (no bullet) as level-1 heading blocks. `#tag` without a space is a tag.
3. **MUST** treat all bytes before the first block line as the pre-block. Parse page properties from YAML front matter at byte 0, `key:: value` lines and `#+key: value` lines. Also hoist `#+key:` directives found anywhere in the file.
4. **MUST** parse properties with mldoc's rule: key = 1+ chars that are not `:` or whitespace, then `":: "` + value or `"::"` at EOL. Recognize them anywhere in a block body, with the first property group winning, but never inside quotes, fences or `#+BEGIN` blocks.
5. **MUST** apply value semantics: quoted strings are verbatim; refs give page sets; comma-splitting only for `alias`/`aliases`/`tags` and `:property/separated-by-commas`; `true`/`false` and integers; the unparsed built-in set is raw.
6. **MUST** preserve unknown and invalid properties verbatim, including order and original text.
7. **MUST** recognize `id:: <uuid>` as the block identity. Never drop or rewrite it. When Bitacora creates a ref or embed to a block without an id, it MUST insert `id:: <uuid>` into that block, after the title line, appended to the existing property group.
8. **MUST** recognize references exactly as in §5.1, including nested page refs, labelled refs, `#[[…]]`, embeds, and refs inside property values. Text inside inline code and fences carries no refs.
9. **MUST** recognize markers only immediately after the bullet or heading hashes and followed by a space. Recognize priorities `[#X]`, `SCHEDULED:`/`DEADLINE:` lines with repeaters `+`, `++`, `.+`, and `:LOGBOOK:` drawers with Logseq's exact CLOCK format (`=>` followed by two spaces).
10. **MUST** write new or edited blocks in canonical form:
    - `-` + space + content;
    - continuation lines prefixed `unit*(depth-1) + "  "`, where unit comes from `:export/bullet-indentation` (default tab);
    - an empty block written as `-`;
    - no blank lines between blocks;
    - the pre-block followed by exactly one blank line;
    - LF line endings.
11. **MUST NOT** rewrite blocks the user did not edit: keep their original bytes, including spaces vs tabs, blank lines, trailing whitespace and CRLF. Logseq will normalize them later if it edits the page, which is acceptable.
12. **MUST** write `collapsed:: true` when the user collapses a block in a way Logseq should see, and remove it on expand. **SHOULD** offer a setting to keep collapse state out of files, so git stays quiet. Logseq treats a missing value as expanded.
13. **MUST** parse the `:PROPERTIES:`…`:END:` drawer form in Markdown blocks. **SHOULD NOT** convert it on read unless the block is edited, which is when Logseq converts it.
14. **SHOULD** keep property order and placement when editing a property value. Change only the affected line, as Logseq's `insert-property` does.
15. **SHOULD** place new SCHEDULED/DEADLINE lines after the title, new properties after the title or SCHEDULED lines, and new `:LOGBOOK:` after the properties, matching §7.
16. **SHOULD** treat the metadata-class properties of §5.4 (`collapsed`, `card-*`, `query-*`, `filters`, `hl-*`, `created-at`/`updated-at`, marker timestamps) and `:LOGBOOK:` as mergeable state in the git merge driver: last-writer-wins per key, and union for CLOCK lines. Treat `id` as identity: keep both on conflict and de-duplicate by uuid.
17. **SHOULD** avoid Logseq's quirks when writing: never emit a non-first block without a bullet; never emit a `heading:: true` first block without a bullet; avoid writing `key:: value` as the first line of the first block unless page properties are intended.
18. **SHOULD** treat `*`, `+` and `1.` lists, tables, quotes and HTML as opaque Markdown body content.
19. **SHOULD** preserve image metadata `{:height N, :width M}` after `![]()`, and write it in EDN `pr-str` style when resizing.
20. **SHOULD** read UTF-8 and compute any offsets in bytes.

---

## 11. Test corpus suggestions

Replicate these fixtures, as input → expected tree/properties/refs and, for writes, expected bytes:

- **Logseq unit tests:**
  - `deps/graph-parser/test/logseq/graph_parser/extract_test.cljs:18-50,81-87` (headings vs bullets, irregular indentation regression #1902);
  - `block_test.cljs:20-111` (duplicate id removal, property extraction matrix);
  - `block_test.cljs:123-163` (block refs via content/properties/page properties, SCHEDULED/DEADLINE);
  - `text_test.cljs:64-115` (property value parsing);
  - `mldoc_test.cljs:47-99,122-137` (Src indentation, definition list, `remove-indentation-spaces`).
- **Frontend tests:**
  - `src/test/frontend/util/property_test.cljs:5-167` (insert/remove property text transforms);
  - `src/test/frontend/fs/diff_merge_test.cljs:58-` (multi-line de-indentation, uuid stability);
  - `marker_test.cljs`, `priority_test.cljs`, `clocktime_test.cljs`;
  - `src/test/frontend/handler/export_test.cljs`.
- **mldoc tests:** `mldoc:test/test_markdown.ml`, especially the property drawer (`:690-760`), CRLF, inline code vs emphasis overlaps (`:760-800`), code block in heading (`:1180-`) and property references (`:1329-1410`); `mldoc:test/test_outline_markdown.ml`.
- **Real graphs:** `src/resources/tutorials/tutorial-*.md` and `dummy-notes-*.md` (column-0 `#+BEGIN_TIP` under bullets, inline hiccup, macros). The **logseq/docs graph at tag v0.9.2** is used by `mldoc_test.cljs:140-168`; its expected AST node counts are listed there and make a good large-scale regression target.
- **Round-trip fixtures to write ourselves:**
  1. 2-space-indented file, no edit → must be byte-identical after Bitacora load+save.
  2. The same file with one block edited → only that block's lines change.
  3. CRLF file; BOM file; tabs+spaces mixture.
  4. Blank lines between blocks and inside blocks.
  5. Front matter page; `title::` page; `#+title:` inside a block.
  6. Fenced code containing `- x`, `foo:: bar`, `[[x]]`, `((uuid))`.
  7. Unclosed fence followed by bullets.
  8. Property edge keys (`a.b.c::`, `empty::`, `my key:: v`, `key::value`, `"quoted"` values, `tags:: a, [[b c]], #d`).
  9. Markdown `:PROPERTIES:` drawer.
  10. Markers with and without text (`- LATER`), priority-only, a marker after `##`.
  11. Repeaters (`+1w`, `++1d`, `.+1m`) and an inactive timestamp.
  12. A LOGBOOK with an open and a closed CLOCK.
  13. All ref forms in §5.1, including `#[[nested [[tag]]]]`.
  14. Embeds and every built-in macro, plus a user macro.
  15. `#+BEGIN_QUERY` with EDN containing `[[` and `"`.
  16. First block `- # Title`, first block `- k:: v`, first block with `heading:: true`.
  17. Collapsed blocks.
  18. Duplicate `id::` across pages.
  19. Image with `{:height …}`.
- **Differential testing:** run `mldoc@1.5.7` under Node (the `parseJson` call with the `default-config` JSON from §1) on the corpus. Compare block segmentation, properties and refs with Bitacora's parser. Also run Logseq's `tree->file-content` logic (it can be ported as a reference implementation) to check that Bitacora-written files re-parse identically.

---

## 12. Open questions

1. ~~Unclosed code fence or `#+BEGIN_X` without an end.~~ **Resolved (mldoc 1.5.7, observed with `tools/mldoc-diff`):** an opener without a closer is plain paragraph text and opens no region, so later bullets still start blocks (`- a\n```\n- b\n- c` gives three blocks). Fixtures: `fixtures/markdown/outline/unclosed-fence.md`, `unclosed-begin-quote.md`; `bitacora-markdown` default is `UnclosedRegion::FallbackToParagraph`. Further observed rules (all in `lines.rs`): a fence is closed by the next line that starts (after any indentation) with *either* ```` ``` ```` or `~~~`, regardless of marker kind or run length (```` ``` ```` is closed by `~~~` and by ```` ```` ````; ```` ```` ```` is closed by ```` ``` ````); a bullet line can itself open a region (`- ```` ` swallows the following bullets); `#+BEGIN_X` is closed by `#+END_X` (case-insensitive prefix match, any indentation), mismatched `#+END_Y` lines are ignored and regions do not nest; front matter (`---` at byte 0 up to the next `---` line) hides its list items, an unclosed one is a horizontal rule.
2. ~~BOM at file start.~~ **Resolved (mldoc 1.5.7):** the BOM is not stripped; it is part of the first line, so `﻿- a` is a paragraph, not a block, `﻿# t` is not a heading and front matter is not recognised after a BOM. Logseq itself does not strip it either (`ufeff` appears in `src/main/frontend/handler/export.cljs:68` only, when exporting). Bitacora mirrors this: the BOM and everything up to the first block line form the pre-block (`fixtures/markdown/outline/bom.md`, `bom-bullet.md`). Block `start_pos` values are UTF-8 byte offsets (the BOM counts 3 bytes).
3. HEAD mldoc (2026) has reworked fast paths (`md_outline.ml`, `try_parse_md_line`). They may differ from 1.5.7 on edge cases: `- LATER` alone is a marker on HEAD (`heading0.ml:22-23`) but not in 1.5.7, and HTML comments differ. Which mldoc version will users' Logseq builds ship? Pin compatibility to 1.5.7 and track upgrades.
4. `insert-property` when the content has *several* property groups may append the new key to every group (`property.cljs:286-302`). Confirm, and decide whether Bitacora should tolerate the duplicates it may encounter.
5. The SRS `card-next-schedule` / `card-last-reviewed` value formats (ISO strings?) matter for merge rules. Check `srs.cljs` before finalizing the merge driver.
6. Org-mode files (`.org`) are part of the file graph too. Out of scope here; decide whether Bitacora opens them read-only.
7. Whiteboards (`whiteboards/*.edn`) and PDF highlight `.edn` files are referenced by `hl-*` properties. Their interplay is covered in [[01-file-graph-layout]].
8. Does Logseq's file watcher re-parse files written by Bitacora and then rewrite them in canonical form without user action? Only when a page is edited in Logseq, per `outliner/file.cljs`. That must be verified with a live Logseq, because it determines whether files written by both apps ping-pong under git. See [[04-editor-outliner-operations]].
