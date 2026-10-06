---
id: BIT-SP-0002
type: spec
title: Logseq graph layout, naming and page lifecycle
status: backlog
author: mcp
labels: [core, compat]
created: 2026-10-06T14:21:35Z
updated: 2026-10-06T19:53:25Z
requirements:
  R1:
    status: backlog
    trace:
      code: [crates/bitacora-core/src/scan.rs#scan_graph]
      tests: [crates/bitacora-core/src/scan.rs#no_trace]
  R2:
    status: backlog
    trace:
      code: [crates/bitacora-core/src/scan.rs#scan_graph]
      tests:
        - crates/bitacora-core/src/scan.rs#golden_scan
        - crates/bitacora-core/src/scan.rs#hidden_prefixes
  R3:
    status: backlog
    trace:
      code:
        - crates/bitacora-config/src/config.rs
        - crates/bitacora-config/src/edn.rs
        - crates/bitacora-config/src/accessors.rs
      tests: [crates/bitacora-config/tests/load.rs]
    verified: {rev: "sha256:4d3240131f1012c0", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R4:
    status: backlog
    trace:
      code:
        - crates/bitacora-config/src/cst.rs
        - crates/bitacora-config/src/edit.rs
      tests: [crates/bitacora-config/tests/edit_golden.rs]
    verified: {rev: "sha256:3efecd8adf33f7c8", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R5:
    status: backlog
    trace:
      code: [crates/bitacora-core/src/naming.rs#derive_title]
      tests: [crates/bitacora-core/src/naming.rs#derive_title_pipeline]
  R6:
    status: backlog
    trace:
      code: [crates/bitacora-core/src/naming.rs#triple_lowbar]
      tests:
        - crates/bitacora-core/src/naming.rs#tlb_encode_vectors
        - crates/bitacora-core/src/naming.rs#tlb_roundtrip
  R7:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/naming.rs#legacy
        - crates/bitacora-core/src/editor/lifecycle.rs#auto_title_preamble
      tests:
        - crates/bitacora-core/src/naming.rs#legacy_vectors
        - crates/bitacora-core/src/naming.rs#title_property_predicate
        - crates/bitacora-core/tests/page_lifecycle.rs#legacy_graph_writes_title_property_for_lossy_names
  R8:
    status: backlog
    trace:
      code: [crates/bitacora-core/src/graph.rs]
      tests:
        - crates/bitacora-core/src/graph.rs#tests
        - crates/bitacora-core/tests/graph_fixtures.rs
  R9:
    status: backlog
    trace:
      code: [crates/bitacora-core/src/graph_path.rs#GraphPath]
      tests:
        - crates/bitacora-core/src/graph_path.rs#normalises
        - crates/bitacora-core/src/scan.rs#nfd_names_are_normalised
  R10:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/journal.rs#detect_journal
        - crates/bitacora-core/src/date.rs#DateFormat
      tests:
        - crates/bitacora-core/src/journal.rs#journal_file_is_detected
        - crates/bitacora-core/src/date.rs#parses_strictly
  R11:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/journal.rs#journal_file_path
        - crates/bitacora-core/src/editor/lifecycle.rs#ensure_today
        - crates/bitacora-core/src/editor/rename.rs#plan_rename
        - crates/bitacora-core/src/queue.rs#Request
        - crates/bitacora-app/src/graph_ops.rs#ensure_today
        - crates/bitacora-app/src/views/workspace.rs#start_day_clock
      tests:
        - crates/bitacora-core/src/journal.rs#file_paths
        - crates/bitacora-core/tests/page_lifecycle.rs#today_is_virtual_until_typed
        - crates/bitacora-core/tests/page_lifecycle.rs#journal_directory_and_file_format_are_configurable
        - crates/bitacora-core/tests/page_rename.rs#journals_are_never_renamed
        - crates/bitacora-app/src/views/workspace.rs#todays_journal_is_ensured_on_open_and_at_the_midnight_rollover
        - crates/bitacora-app/src/graph_ops.rs#todays_journal_is_virtual_until_it_has_content
  R12:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/graph.rs
        - crates/bitacora-core/src/editor/lifecycle.rs#open_page
        - crates/bitacora-core/src/editor/model.rs#is_virtual
      tests:
        - crates/bitacora-core/tests/graph_fixtures.rs
        - crates/bitacora-core/tests/page_lifecycle.rs
  R13:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/rename.rs#rewrite_refs
        - crates/bitacora-core/src/editor/rename.rs#plan_rename
        - crates/bitacora-core/src/editor/op.rs#RenamePage
        - crates/bitacora-core/src/editor/op.rs#EditFile
        - crates/bitacora-runtime/src/rename_lookup.rs
      tests:
        - crates/bitacora-core/tests/page_rename.rs
        - crates/bitacora-core/src/rename.rs#tests
        - crates/bitacora-runtime/tests/rename.rs
  R14:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/recycle.rs
        - crates/bitacora-core/src/editor/flush.rs#recycle
        - crates/bitacora-core/src/editor/fsio.rs#recycle
        - crates/bitacora-core/src/editor/op.rs#DeleteAsset
        - crates/bitacora-core/src/editor/rename.rs#plan_merge
        - crates/bitacora-app/src/graph_ops.rs#delete_page
        - crates/bitacora-app/src/graph_ops.rs#delete_asset
        - crates/bitacora-app/src/graph_ops.rs#favorites_remove
      tests:
        - crates/bitacora-core/tests/page_lifecycle.rs#deleting_a_page_recycles_its_file
        - crates/bitacora-core/tests/page_lifecycle.rs#deleting_an_asset_recycles_it_and_undo_restores_it
        - crates/bitacora-core/tests/page_lifecycle.rs#fs_store_recycles_with_rename_and_overwrites
        - crates/bitacora-core/tests/page_rename.rs#merge_appends_blocks_recycles_the_source_and_rewrites_refs
        - crates/bitacora-app/src/graph_ops.rs#deleting_a_page_recycles_the_file_and_drops_the_favorite
        - crates/bitacora-app/src/graph_ops.rs#an_unreferenced_asset_is_recycled_and_a_referenced_one_is_kept
  R15:
    status: backlog
  R16:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/scan.rs#decode_text
        - crates/bitacora-core/src/editor/model.rs#serialize_checked
      tests:
        - crates/bitacora-core/src/scan.rs#bom_reader
        - crates/bitacora-core/tests/page_lifecycle.rs#file_name_keeps_title_case_and_output_is_clean
  R17:
    status: backlog
  R18:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/graph.rs
        - crates/bitacora-core/src/editor/cmd.rs#set_page_property
      tests:
        - crates/bitacora-core/src/graph.rs#tests
        - crates/bitacora-core/tests/page_lifecycle.rs#page_property_goes_into_the_pre_block
        - crates/bitacora-core/tests/page_lifecycle.rs#page_property_in_front_matter_page_stays_front_matter
  R19:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/new_graph.rs
        - crates/bitacora-config/src/default_config.edn
      tests:
        - crates/bitacora-core/tests/page_lifecycle.rs#new_graph_layout_and_config
        - crates/bitacora-config/tests/load.rs#default_config_text_parses
  R20:
    status: backlog
    trace:
      code: [crates/bitacora-core/src/graph.rs]
      tests: [crates/bitacora-core/src/graph.rs#tests]
---

## Purpose
Bitacora uses exactly the same folders, file names, journals, assets, config and page lifecycle behaviour as Logseq file graphs.

## Scope
Graph discovery and ignore rules, `config.edn`, title ↔ file name mapping (triple-lowbar and legacy), page identity, journals, lazy file creation, rename cascade, delete to `.recycle`, backups, assets. Source: [[01-file-graph-layout]]. Implemented by BIT-EP-0004 and BIT-EP-0009.

## Requirements

### BIT-SP-0002.R1 — Graph folder is the source of truth; unowned files are preserved byte-for-byte

Bitacora SHALL treat the graph directory as the only source of truth and SHALL NOT create sidecar files inside it, except under a dot-directory (e.g. `.bitacora/`) that Logseq ignores. It SHALL preserve byte-for-byte every file it does not understand or own: `.org` files, `whiteboards/*.edn`, `draws/*.excalidraw`, `.tldr`, `logseq/custom.css`, `logseq/custom.js`, `logseq/export.css`, the `logseq/bak/`, `logseq/.recycle/` and `logseq/version-files/` folders, and any `.git` file or directory (including a `.git` file containing `gitdir: …`).

#### Scenario: Opening a graph leaves no traces
- GIVEN a Logseq graph with `pages/`, `journals/` and `logseq/config.edn`
- WHEN Bitacora opens, indexes and closes it without user edits
- THEN `git status` in the graph shows no new, modified or deleted files

#### Scenario: Whiteboard and drawing files untouched
- GIVEN `whiteboards/Plan.edn` and `draws/2025-11-14-10-30-00.excalidraw`
- WHEN Bitacora indexes the graph and the user edits an unrelated page
- THEN both files keep their original bytes and modification times

#### Scenario: Separate git dir file preserved
- GIVEN the graph root contains a `.git` file with `gitdir: /home/u/.logseq/git/_home_u_notes/.git`
- WHEN Bitacora opens the graph
- THEN the `.git` file is neither deleted nor rewritten

### BIT-SP-0002.R2 — Logseq ignore rules and :hidden prefixes when scanning the graph

The graph scanner SHALL implement Logseq's ignore rules (`deps/common/src/logseq/common/graph.cljs:44-113`): skip symlinks and every path with a segment starting with `.`, `node_modules/`, `.DS_Store`, `logseq/bak/`, `logseq/.recycle/`, `logseq/version-files/`, `logseq/graphs-txid.edn` and `logseq/pages-metadata.edn`; keep only the extensions `org markdown md edn json js css excalidraw tldr`; and pass only `md`, `markdown`, `org`, `edn` and `css` to parsing. It SHALL apply each `:hidden` config entry as a root-relative path prefix (a leading `/` optional; not a glob), as in `deps/common/src/logseq/common/config.cljs:5-25`. Files SHALL be ordered as in `filter-files` (journals reverse-sorted first, then built-ins, then the rest).

#### Scenario: Backup and recycle folders ignored
- GIVEN `logseq/bak/pages/foo/2025-11-14T09_30_12.345Z.Desktop.md` and `logseq/.recycle/pages_old.md`
- WHEN the graph is scanned
- THEN neither file produces a page

#### Scenario: Hidden prefix
- GIVEN `:hidden ["/archived" "test.md"]` and files `archived/x.md`, `archived/sub/y.md`, `test.md`, `pages/archived.md`
- WHEN the graph is scanned
- THEN `archived/x.md`, `archived/sub/y.md` and `test.md` are skipped, while `pages/archived.md` is indexed

#### Scenario: Hidden path segment
- GIVEN `pages/.drafts/a.md` and `pages/.b.md`
- WHEN the graph is scanned
- THEN both files are skipped

### BIT-SP-0002.R3 — config.edn loading and merging with legacy name-format default

Bitacora SHALL read `logseq/config.edn` as EDN and merge it over the optional global `~/.logseq/config/config.edn` and the built-in defaults (`:feature/enable-search-remove-accents? true`, `:ui/auto-expand-block-refs? true`, `:file/name-format :legacy`), with later sources winning and maps shallow-merged (`state.cljs:335-383`). A config without `:file/name-format` SHALL mean `:legacy`. Invalid EDN or duplicate keys SHALL be reported without aborting the graph load. Bitacora MAY ignore everything else under `~/.logseq` and SHALL never write the global config.

#### Scenario: Missing name format means legacy
- GIVEN `logseq/config.edn` containing `{:meta/version 1}` and no global config
- WHEN the effective config is computed
- THEN `:file/name-format` is `:legacy`

#### Scenario: Graph config wins and maps shallow-merge
- GIVEN a global config `{:macros {"a" "x"} :preferred-workflow :todo}` and graph config `{:macros {"b" "y"} :preferred-workflow :now}`
- WHEN the effective config is computed
- THEN `:preferred-workflow` is `:now` and `:macros` is `{"a" "x" "b" "y"}`

#### Scenario: Duplicate keys
- GIVEN a graph config containing `:hidden []` twice
- WHEN the graph is opened
- THEN a config error is reported and defaults plus the global config are used

### BIT-SP-0002.R4 — Surgical, comment-preserving config.edn edits

Bitacora SHALL edit `logseq/config.edn` surgically, in the style of `borkdude.rewrite-edn` (`handler/config.cljs:9-31`): only the targeted key's value node changes, and all comments, whitespace, ordering, commented-out keys and unknown keys elsewhere in the file are preserved byte-for-byte. Adding a missing key SHALL append it inside the top-level map without reformatting existing entries.

#### Scenario: Update favorites keeps comments
- GIVEN a long, heavily commented `config.edn` with `:favorites []` (as a user's Logseq-created graph has; tests use our own commented fixture, never a copy of Logseq's template — ADR-015)
- WHEN Bitacora adds the favorite `"Projects"`
- THEN the only diff is `:favorites []` → `:favorites ["Projects"]`

#### Scenario: Add a missing key
- GIVEN a config without `:default-home`
- WHEN Bitacora sets `:default-home {:page "Home"}`
- THEN the key is inserted before the closing `}` of the top-level map and no other line changes

### BIT-SP-0002.R5 — Page title derivation order

Bitacora SHALL derive a page title in Logseq's priority order (`extract.cljc:30-63`): (1) a path starting with `pages/contents.` → `Contents`; (2) the `title` property (case-insensitive key) of the first AST element when it is a property pre-block or YAML front matter; (3) the file body — base name with everything after the **last** dot removed (`graph_parser/util.cljs:204-209`) — decoded with the active `:file/name-format`. The derived title SHALL then be checked for journal-ness. A mismatch between `title::` and file name SHALL be tolerated.

#### Scenario: Contents page
- GIVEN `pages/contents.md`
- WHEN the title is derived
- THEN it is `Contents`

#### Scenario: title:: overrides file name
- GIVEN `pages/foo.md` whose first lines are `title:: Bar`
- WHEN the title is derived
- THEN the page is `Bar` and its file stays `pages/foo.md`

#### Scenario: Last-dot split
- GIVEN a triple-lowbar graph and `pages/Version 1.0.md` without `title::`
- WHEN the title is derived
- THEN the title is `Version 1.0`

### BIT-SP-0002.R6 — Bit-exact :triple-lowbar file name codec

Bitacora SHALL implement `tri-lb-file-name-sanity` (`src/main/frontend/util/fs.cljs:125-135`) and `tri-lb-title-parsing` (`deps/graph-parser/src/logseq/graph_parser/util.cljs:153-160`) bit-exactly: strip one leading and one trailing `/` and NFC-normalise; pre-escape `%[0-9a-fA-F]{2}` to `%25XX`; percent-encode runs of ``: * ? " < > | # \`` (with `*` → `%2A`); encode a leading `.` as `%2E`; append `/` to Windows reserved names (`CON PRN AUX NUL COM1-9 LPT1-9`, case-sensitive) and bodies ending in `.`; escape `___`, `_/`, `/_` with `%5F` and then map `/` → `___`. Decoding SHALL replace `___` with `/` left-to-right, decode each `%XX` token individually in a single pass (leaving undecodable tokens), and drop empty namespace segments. The examples in [[01-file-graph-layout]] §3.2 and `src/test/frontend/db/name_sanity_test.cljs` SHALL be test vectors.

#### Scenario: Namespaces and reserved characters
- GIVEN titles `Projects/Bitacora/Design`, `What? A: B`, `C#`, `aa?#/bbb/ccc`
- WHEN encoded
- THEN file bodies are `Projects___Bitacora___Design`, `What%3F A%3A B`, `C%23`, `aa%3F%23___bbb___ccc` and each decodes back to its title

#### Scenario: Percent handling
- GIVEN titles `50% done` and the literal `a%2Fb`
- WHEN encoded
- THEN file bodies are `50% done` and `a%252Fb`, and decoding `a%252Fb` yields `a%2Fb`

#### Scenario: Underscore disambiguation
- GIVEN titles `foo___bar`, `a_/b`, `a/_b`, `a__/bbb/ccc`
- WHEN encoded
- THEN file bodies are `foo%5F%5F%5Fbar`, `a%5F___b`, `a___%5Fb`, `a_%5F___bbb___ccc`

#### Scenario: Hidden, reserved and trailing-dot names
- GIVEN titles `.hidden`, `CON`, `con`, `ends with.`
- WHEN encoded
- THEN file bodies are `%2Ehidden`, `CON___`, `con`, `ends with.___`

### BIT-SP-0002.R7 — Legacy file name codec and automatic title:: in legacy graphs

Bitacora SHALL implement legacy decoding (`graph_parser/util.cljs:244-247`): replace every `.` with `/`, then URL-decode the whole string, falling back to the raw string on error. It SHALL implement legacy encoding (`util/fs.cljs:168-181`): URL-encode runs of `[\ # | %]`, runs of `[: * ? " < > |]` and `/` (→ `%2F`), with `*` → `%2A`. In legacy graphs, when creating a page whose title would not survive an encode→decode round-trip or whose file name would contain reserved characters, Bitacora SHALL write `title:: <Original Title>` as the first (pre-block) line, as `fs-util/create-title-property?` does (`fs.cljs:198-206`).

#### Scenario: Legacy dot decoding
- GIVEN a legacy graph and `pages/Version 1.0.md` without `title::`
- WHEN the title is derived
- THEN the page is the namespace page `Version 1/0`

#### Scenario: Legacy namespace encoding
- GIVEN a legacy graph
- WHEN the page `Projects/Bitacora` is created with content
- THEN its file is `pages/Projects%2FBitacora.md`

#### Scenario: Auto title property
- GIVEN a legacy graph
- WHEN the page `Version 1.0` is created with the block `- hello`
- THEN the file starts with `title:: Version 1.0` followed by a blank line and `- hello`

### BIT-SP-0002.R8 — Case-insensitive NFC page identity and duplicate-title handling

Bitacora SHALL key pages by `page-name-sanity-lc`: lower-case, one leading and one trailing `/` stripped, Unicode NFC (`graph_parser/util.cljs:134-142,162-165`). It SHALL preserve the original case for display and file names. When two files map to the same page key, it SHALL keep the first file in Logseq's `filter-files` order, skip the others and report the conflict ("The file X will be skipped because another file Y has the same page title") instead of merging them.

#### Scenario: Case-insensitive identity
- GIVEN references `[[Foo]]` and `[[foo]]`
- WHEN the graph is indexed
- THEN both resolve to one page whose display name is the case of the backing file or first creation

#### Scenario: NFC identity
- GIVEN a file `pages/Café.md` stored in NFD and a reference `[[café]]` in NFC
- WHEN indexed
- THEN the reference resolves to that page

#### Scenario: Duplicate titles
- GIVEN `pages/Foo.md` and `pages/sub/foo.md` on a case-sensitive file system
- WHEN the graph is loaded
- THEN only the first file in load order backs page `foo`, and a duplicate-title warning names both paths

### BIT-SP-0002.R9 — Graph-relative, NFC-normalised paths with / separators

Bitacora SHALL NFC-normalise every path before storing or writing it, and SHALL store paths relative to the graph root with `/` separators on every OS (`graph_parser/util.cljs:24-28`, `common/graph.cljs:36-42`). Files created by Bitacora SHALL use NFC file names.

#### Scenario: Windows separators
- GIVEN a graph on Windows with the file `pages\sub\foo.md`
- WHEN it is indexed
- THEN its stored path is `pages/sub/foo.md`

#### Scenario: NFD file names on macOS
- GIVEN a file whose name was returned by the OS in NFD form `Café.md`
- WHEN it is indexed
- THEN its stored path is `pages/Café.md` in NFC and its title is `Café`

### BIT-SP-0002.R10 — Journal detection, journal-day and title rendering

Bitacora SHALL detect journals by title, not directory: the derived title is capitalised word-by-word and parsed with `[:journal/page-title-format, "MMM do, yyyy", "yyyy-MM-dd", "yyyy_MM_dd"]` (`date_time_util.cljs:15-41`, `block.cljs:273-284`). For a journal it SHALL compute `journal-day = yyyyMMdd` and render the display title with `:journal/page-title-format` (default `MMM do, yyyy`), supporting Joda-style patterns including the ordinal `do`. The page key SHALL be the lower-cased rendered title.

#### Scenario: Default file name
- GIVEN `journals/2025_11_14.md` and default config
- WHEN indexed
- THEN the page is a journal with title `Nov 14th, 2025`, key `nov 14th, 2025` and journal-day `20251114`

#### Scenario: Journal outside journals/
- GIVEN `pages/2024_01_01.md`
- WHEN indexed
- THEN it is a journal page for `20240101`

#### Scenario: Custom title format
- GIVEN `:journal/page-title-format "yyyy-MM-dd"` and `journals/2025_11_14.md`
- WHEN indexed
- THEN the title is `2025-11-14` and `[[2025-11-14]]` resolves to it

### BIT-SP-0002.R11 — Journal file naming; journals are never renamed

Bitacora SHALL name new journal files `<:journals-directory>/<format(:journal/file-name-format | "yyyy_MM_dd")>.md` (`date.cljs:195-211`), using `:journals-directory` (default `journals`). Changing `:journal/file-name-format` SHALL NOT be retroactive. Bitacora SHALL never rename an existing journal file, neither on page rename nor on filename-format conversion.

#### Scenario: Default journal name
- GIVEN default config and the date 2025-11-14
- WHEN the journal gets its first content
- THEN the file `journals/2025_11_14.md` is created

#### Scenario: Custom directory and format
- GIVEN `:journals-directory "daily"` and `:journal/file-name-format "yyyy-MM-dd"`
- WHEN the 2025-11-14 journal gets content
- THEN the file is `daily/2025-11-14.md`

#### Scenario: Journal rename leaves file alone
- GIVEN `journals/2025_11_14.md`
- WHEN a rename of the journal page is attempted
- THEN the file path does not change

### BIT-SP-0002.R12 — Lazy page file creation; no files for virtual pages

Bitacora SHALL create new non-journal pages at `<:pages-directory>/<encoded title>.md` (default `pages/`, encoding per the active `:file/name-format`, file name keeping the case of the original title). It SHALL NOT create a file until the page has non-blank content (`modules/file/core.cljs:115-166`), and SHALL NOT create files for alias pages, namespace parents, property pages or merely referenced pages. It SHALL NOT create a `.md` file for a page that is already backed by an `.org` file.

#### Scenario: Reference does not create a file
- GIVEN a block containing `[[New Page]]`
- WHEN the user navigates to `New Page` and leaves without typing
- THEN no `pages/New Page.md` exists

#### Scenario: First content creates the file
- GIVEN the virtual page `Projects/Bitacora` in a triple-lowbar graph
- WHEN the user types `hello` into its first block
- THEN `pages/Projects___Bitacora.md` is created with content `- hello` and no file is created for `Projects`

#### Scenario: Org-backed page
- GIVEN `pages/Notes.org`
- WHEN the user references `[[Notes]]`
- THEN no `pages/Notes.md` is created

### BIT-SP-0002.R13 — Page rename cascade

On page rename Bitacora SHALL follow `rename-page-aux` (`handler/page.cljs:450-645`): rename the file in place (same directory and extension, new body = encoded new title); rewrite `title::` (or front-matter `title:`) when the first block mentions the old name; rewrite references across all files — `[[Old Name]]` → `[[New Name]]`, `#Old` → `#New` or `#[[New Name]]` when the new name contains whitespace (case-insensitive, word boundaries), and property keys `old::` → `new-name::` (lower-cased, spaces → `-`) plus refs in property values; rename namespace children (`a/x` when `a` is renamed, replacing the prefix once); rename pages whose titles contain `[[old]]`; update `:favorites` and `:default-home :page` in `config.edn`. A case-only rename SHALL update the title and file name. When the target page exists, the pages SHALL be merged: source blocks moved to the end of the target, refs rewritten, source file moved to `logseq/.recycle/`. Journals SHALL NOT be renamed on disk.

#### Scenario: Simple rename
- GIVEN `pages/Old.md` and a journal block `see [[Old]] and #Old`
- WHEN the user renames `Old` to `New Idea`
- THEN the file becomes `pages/New Idea.md` and the block reads `see [[New Idea]] and #[[New Idea]]`

#### Scenario: Property key rewrite
- GIVEN a block with the property line `old:: value` referring to page `Old`
- WHEN `Old` is renamed to `New Idea`
- THEN the line becomes `new-idea:: value`

#### Scenario: Namespace children
- GIVEN pages `a`, `a/x` and `a/y` with files `pages/a.md`, `pages/a___x.md`, `pages/a___y.md`
- WHEN `a` is renamed to `b`
- THEN the files become `pages/b.md`, `pages/b___x.md`, `pages/b___y.md` and refs `[[a/x]]` become `[[b/x]]`

#### Scenario: Merge on collision
- GIVEN pages `Foo` (2 blocks) and `Bar` (1 block) both with files
- WHEN `Foo` is renamed to `Bar`
- THEN `pages/Bar.md` holds its block followed by Foo's 2 blocks, `pages/Foo.md` is moved to `logseq/.recycle/pages_Foo.md`, and `[[Foo]]` refs become `[[Bar]]`

#### Scenario: Config updated
- GIVEN `:favorites ["Old"]` and `:default-home {:page "Old"}`
- WHEN `Old` is renamed to `New`
- THEN both entries read `"New"` and the rest of `config.edn` is unchanged

### BIT-SP-0002.R14 — Delete to logseq/.recycle, never unlink

Bitacora SHALL delete pages and assets by moving the file to `logseq/.recycle/<relative path with "/" and "\" replaced by "_">` (`frontend/fs.cljs:77-81`, `electron/handler.cljs:51-66`), overwriting an existing recycled file with the same name, and SHALL never unlink user files. References to the deleted page SHALL NOT be rewritten (they become dangling refs to a file-less page). The page SHALL be removed from `:favorites`. If another page aliases the deleted page, the page entity SHALL be kept without attributes.

#### Scenario: Delete a page
- GIVEN `pages/foo.md`
- WHEN the user deletes page `foo`
- THEN the file exists at `logseq/.recycle/pages_foo.md` and `pages/foo.md` no longer exists

#### Scenario: Nested path
- GIVEN `pages/sub/bar.md`
- WHEN page `bar` is deleted
- THEN the file is moved to `logseq/.recycle/pages_sub_bar.md`

#### Scenario: Dangling references kept
- GIVEN a block `see [[foo]]`
- WHEN page `foo` is deleted
- THEN the block text is unchanged

### BIT-SP-0002.R15 — Asset naming and linking on paste/drop

Bitacora SHALL save pasted or dropped files to `assets/<stem>_<epoch-ms>_<index><ext>` (`handler/editor.cljs:1392-1454`), where the stem has ` `, `%` and `/` replaced by `_` and runs of `_` collapsed to one, and `<ext>` is `extname` (falling back to the multi-dot extension when unknown). It SHALL link the file with a path relative to the page file (`../assets/<name>` for `pages/x.md` and `journals/x.md`, `../../assets/<name>` for `pages/sub/x.md`, base `pages/_.md` when the page has no file yet), using `![<original file name>](…)` for images, audio, video and PDF, and `[<original file name>](…)` otherwise. Asset links starting with `../assets`, `./assets`, `/assets` or `assets` SHALL resolve relative to the file with a fallback to `<root>/assets`, and `@alias/…` links SHALL be preserved verbatim.

#### Scenario: Screenshot paste
- GIVEN the file `Screen Shot 2024.png` pasted at epoch ms `1731580000000` into `journals/2025_11_14.md`
- WHEN the paste completes
- THEN `assets/Screen_Shot_2024_1731580000000_0.png` exists and the block contains `![Screen Shot 2024.png](../assets/Screen_Shot_2024_1731580000000_0.png)`

#### Scenario: Non-image file
- GIVEN `report 50%.docx` dropped as the second file at `1731580000000` into `pages/sub/x.md`
- WHEN the drop completes
- THEN `assets/report_50_1731580000000_1.docx` exists and the link is `[report 50%.docx](../../assets/report_50_1731580000000_1.docx)`

### BIT-SP-0002.R16 — UTF-8, LF and BOM handling

Bitacora SHALL read and write UTF-8, SHALL write LF line endings in files it creates, and SHALL NOT add a BOM. It SHOULD strip a leading U+FEFF from the parsed text only (so it does not end up in the first property or block), without rewriting the file unless the page is edited; existing line endings of untouched content are preserved per BIT-SP-0001.

#### Scenario: New file encoding
- GIVEN a new page `Notes` with block `héllo`
- WHEN the file is written
- THEN `pages/Notes.md` is UTF-8 without BOM and contains `- héllo` with LF endings only

#### Scenario: BOM file read
- GIVEN `pages/bom.md` starting with bytes `EF BB BF` followed by `title:: Bom\n\n- a`
- WHEN it is indexed
- THEN the page title is `Bom` (no U+FEFF in the key) and the file bytes are unchanged

### BIT-SP-0002.R17 — External change protection and Logseq-layout backups

Bitacora SHALL NOT rewrite a file whose on-disk content (trimmed) differs from Bitacora's last-known content without first merging, backing up or asking the user (Logseq's `:file/not-matched-from-disk`, `fs/node.cljs:44-50`). It SHOULD write backups before destructive overwrites in Logseq's layout `logseq/bak/<dir>/<stem>/<ISO-8601 timestamp with ":" → "_">.<Client>.<ext>`, keeping the newest 6 per directory (`backup_file.cljs:27-34`). It SHOULD watch the graph directory and re-parse files changed externally (Logseq, git, sync), using trimmed-content comparison to drop no-op events and ignoring a journal whose trimmed content equals `-` or the default template.

#### Scenario: Backup layout
- GIVEN `pages/foo.md` changed on disk after Bitacora read it
- WHEN the user's pending edit is applied after confirmation
- THEN the previous disk content is saved as `logseq/bak/pages/foo/2025-11-14T09_30_12.345Z.Bitacora.md` and at most 6 files remain in `logseq/bak/pages/foo/`

#### Scenario: Whitespace-only external change
- GIVEN an external tool appends a trailing newline to `pages/foo.md`
- WHEN the watcher event arrives
- THEN no re-parse conflict or backup is produced

### BIT-SP-0002.R18 — Front matter preservation and page-property writing format

Bitacora SHOULD keep YAML front matter as-is when present and, when adding or changing a page property in a front-matter page, use `key: value` syntax inside it. It SHALL never create front matter; new page properties SHOULD be written as `key:: value` lines in the first, un-bulleted block followed by one blank line. Page-level properties (`title`, `alias`/`aliases`, `tags`, `public`, `filters`, `icon`, `exclude-from-graph-view`) SHALL be read from that block with keys lower-cased; `alias` and `tags` SHALL create DB-only pages (no files), dropping blank aliases and aliases equal to the page itself.

#### Scenario: Add property to a plain page
- GIVEN `pages/x.md` containing `- a`
- WHEN the user adds page property `tags:: demo`
- THEN the file becomes `"tags:: demo\n\n- a"`

#### Scenario: Front matter page
- GIVEN a file starting `---\ntitle: Front\n---\n- a`
- WHEN the user adds page property `icon` = `🚀`
- THEN the front matter becomes `---\ntitle: Front\nicon: 🚀\n---` and the rest is unchanged

#### Scenario: Alias pages have no files
- GIVEN `alias:: Mine, [[My page alias]]` on page `My Page`
- WHEN indexed
- THEN `[[Mine]]` resolves to `My Page` and no `pages/Mine.md` is created

### BIT-SP-0002.R19 — New graph creation with a Bitacora default config Logseq accepts

When creating a new graph, Bitacora SHOULD create `pages/`, `journals/`, `assets/`, `logseq/config.edn`, `logseq/custom.css` (empty), `logseq/.recycle/` and `pages/contents.md` (content `-`). `logseq/config.edn` SHALL be Bitacora's own default config, written by us (ADR-015: no text or comments copied from Logseq's `src/resources/templates/config.edn`), containing only the keys and values needed for Logseq to open the graph as a current file graph, including `:meta/version 1` and `:file/name-format :triple-lowbar`. Byte-identity with Logseq's template (and therefore Logseq's MD5 "default config" detection, `config.cljs:349-353`) is a non-goal.

#### Scenario: Default config contents
- GIVEN an empty directory
- WHEN the user creates a new graph there
- THEN `logseq/config.edn` is Bitacora's default config, parses as EDN and contains `:file/name-format :triple-lowbar`

#### Scenario: Logseq opens the new graph
- GIVEN a graph created by Bitacora
- WHEN it is opened in Logseq 0.10.15
- THEN Logseq shows no conversion prompt and indexes `Contents` and today's journal normally

### BIT-SP-0002.R20 — Org pages indexed read-only

Bitacora SHOULD index `.org` pages read-only, deriving at least the title (from `#+TITLE:` or the file name) and refs so that `[[…]]` references resolve, and SHALL never rewrite `.org` files. The format SHALL be decided per file by extension (`.md`/`.markdown` → Markdown, `.org` → Org, `graph_parser/util.cljs:188-219`); other extensions under `pages/` (e.g. `.adoc`) SHALL be ignored and left alone.

#### Scenario: Org title resolves refs
- GIVEN `pages/Meeting.org` starting with `#+TITLE: Weekly Meeting` and a Markdown block `see [[Weekly Meeting]]`
- WHEN indexed
- THEN the reference resolves to the org page and the page is shown read-only

#### Scenario: Unknown extension ignored
- GIVEN `pages/spec.adoc`
- WHEN the graph is scanned
- THEN no page is created and the file is untouched
