---
id: BIT-SP-0001
type: spec
title: Logseq Markdown compatibility
status: backlog
author: mcp
labels: [markdown, compat]
created: 2026-10-06T14:21:20Z
updated: 2026-10-06T18:25:16Z
requirements:
  R1:
    status: backlog
    trace:
      code:
        - crates/bitacora-markdown/src/lines.rs
        - crates/bitacora-markdown/src/outline.rs#split
        - crates/bitacora-markdown/src/tree.rs#build_tree
      tests:
        - crates/bitacora-markdown/src/lines.rs
        - crates/bitacora-markdown/src/tree.rs
        - crates/bitacora-markdown/tests/outline_fixtures.rs
        - crates/bitacora-markdown/tests/outline_roundtrip.rs
    verified: {rev: "sha256:43a2e4393aba4447", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R2:
    status: backlog
    trace:
      code:
        - crates/bitacora-markdown/src/lines.rs#classify
        - crates/bitacora-markdown/src/outline.rs#BlockKind
      tests:
        - crates/bitacora-markdown/src/lines.rs
        - crates/bitacora-markdown/src/outline.rs
        - crates/bitacora-markdown/tests/outline_fixtures.rs
    verified: {rev: "sha256:6f6f78c05272e8ee", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R3:
    status: backlog
    trace:
      code:
        - crates/bitacora-markdown/src/page_props.rs#page_properties
        - crates/bitacora-markdown/src/outline.rs#pre_block_content
      tests:
        - crates/bitacora-markdown/src/page_props.rs
        - crates/bitacora-markdown/tests/page_props_fixtures.rs
        - crates/bitacora-markdown/tests/mldoc_corpus.rs
    verified: {rev: "sha256:11a27ac7140bd97e", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R4:
    status: backlog
    trace:
      code: [crates/bitacora-markdown/src/properties/scan.rs]
      tests: [crates/bitacora-markdown/src/properties/scan.rs]
    verified: {rev: "sha256:2114df9d7f105415", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R5:
    status: backlog
    trace:
      code: [crates/bitacora-markdown/src/properties/value.rs]
      tests: [crates/bitacora-markdown/src/properties/value.rs]
    verified: {rev: "sha256:b3550e320c7e109e", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R6:
    status: backlog
    trace:
      code: [crates/bitacora-markdown/src/properties/scan.rs#PropertyScan]
      tests: [crates/bitacora-markdown/src/properties/scan.rs]
    verified: {rev: "sha256:3a6a4aaaf70e537c", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R7:
    status: backlog
    trace:
      code: [crates/bitacora-markdown/src/edit/identity.rs]
      tests:
        - crates/bitacora-markdown/src/edit/identity.rs
        - crates/bitacora-markdown/tests/edit_golden.rs
    verified: {rev: "sha256:449f318287e0efce", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R8:
    status: backlog
    trace:
      code:
        - crates/bitacora-markdown/src/inline/scan.rs#scan
        - crates/bitacora-markdown/src/inline/refs.rs#collect
        - crates/bitacora-markdown/src/block.rs#analyze
      tests:
        - crates/bitacora-markdown/src/inline/scan.rs
        - crates/bitacora-markdown/src/inline/refs.rs
        - crates/bitacora-markdown/tests/inline_fixtures.rs
        - crates/bitacora-markdown/tests/inline_robustness.rs
        - crates/bitacora-markdown/tests/mldoc_corpus.rs
    verified: {rev: "sha256:52ec017df5ed9cc6", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R9:
    status: backlog
    trace:
      code: [crates/bitacora-markdown/src/tasks/head.rs#parse_head]
      tests:
        - crates/bitacora-markdown/src/tasks/head.rs
        - crates/bitacora-markdown/tests/tasks_fixtures.rs
        - crates/bitacora-markdown/tests/mldoc_corpus.rs
    verified: {rev: "sha256:9e29ce8a0fa8ffb2", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R10:
    status: backlog
    trace:
      code:
        - crates/bitacora-markdown/src/tasks/timestamp.rs
        - crates/bitacora-markdown/src/tasks/drawer.rs
      tests:
        - crates/bitacora-markdown/src/tasks/timestamp.rs
        - crates/bitacora-markdown/src/tasks/drawer.rs
        - crates/bitacora-markdown/tests/tasks_fixtures.rs
    verified: {rev: "sha256:68d2d736db97f3ac", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R11:
    status: backlog
    trace:
      code: [crates/bitacora-markdown/src/canonical.rs#write_block]
      tests:
        - crates/bitacora-markdown/src/canonical.rs
        - crates/bitacora-markdown/tests/serializer.rs#canonical_example_is_rebuilt_byte_for_byte
    verified: {rev: "sha256:f29d355d69e7e795", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R12:
    status: backlog
    trace:
      code:
        - crates/bitacora-markdown/src/doc.rs#Document
        - crates/bitacora-markdown/src/serialize.rs#serialize
      tests:
        - crates/bitacora-markdown/src/serialize.rs
        - crates/bitacora-markdown/tests/serializer.rs
        - crates/bitacora-markdown/tests/roundtrip_suite.rs
    verified: {rev: "sha256:373156ca4f0054c5", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R13:
    status: backlog
    trace:
      code: [crates/bitacora-markdown/src/edit/state.rs]
      tests:
        - crates/bitacora-markdown/src/edit/state.rs
        - crates/bitacora-markdown/tests/edit_golden.rs
    verified: {rev: "sha256:3973a85335e0dedd", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R14:
    status: backlog
    trace:
      code:
        - crates/bitacora-markdown/src/properties/drawer.rs
        - crates/bitacora-markdown/src/canonical.rs#convert_drawers
      tests:
        - crates/bitacora-markdown/src/properties/drawer.rs
        - crates/bitacora-markdown/src/canonical.rs
        - crates/bitacora-markdown/src/serialize.rs
    verified: {rev: "sha256:f55a279a9e56d87d", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R15:
    status: backlog
    trace:
      code: [crates/bitacora-markdown/src/edit/properties.rs]
      tests:
        - crates/bitacora-markdown/src/edit/properties.rs
        - crates/bitacora-markdown/tests/edit_golden.rs
    verified: {rev: "sha256:f8976f0b9dde05ba", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R16:
    status: backlog
    trace:
      code: [crates/bitacora-markdown/src/classify.rs]
      tests: [crates/bitacora-markdown/src/classify.rs]
    verified: {rev: "sha256:0e83fe578bc360d8", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R17:
    status: backlog
    trace:
      code:
        - crates/bitacora-markdown/src/canonical.rs#IndentUnit
        - crates/bitacora-markdown/src/canonical.rs#Eol
      tests:
        - crates/bitacora-markdown/src/canonical.rs
        - crates/bitacora-markdown/src/serialize.rs
    verified: {rev: "sha256:ed32a2f9899b2cb0", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R18:
    status: backlog
    trace:
      code: [crates/bitacora-markdown/src/image_meta.rs]
      tests:
        - crates/bitacora-markdown/src/image_meta.rs
        - crates/bitacora-markdown/tests/edit_golden.rs#image_resize
    verified: {rev: "sha256:de663ee084fbb5ec", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R19:
    status: backlog
    trace:
      code:
        - crates/bitacora-markdown/src/lines.rs
        - crates/bitacora-markdown/src/outline.rs
        - crates/bitacora-markdown/src/properties/scan.rs
      tests:
        - crates/bitacora-markdown/src/outline.rs
        - crates/bitacora-markdown/src/properties/scan.rs
        - crates/bitacora-markdown/tests/outline_fixtures.rs
        - crates/bitacora-markdown/tests/outline_roundtrip.rs
        - crates/bitacora-markdown/tests/roundtrip_suite.rs
    verified: {rev: "sha256:bb8a523231ec7ede", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
---

## Purpose
Bitacora reads and writes Logseq 0.10.x Markdown pages so that both apps can edit the same graph safely.

## Scope
Outline/block structure, properties, references, task syntax, drawers, macros, lossless round-trip and canonical serialization of edited blocks, content-vs-metadata property classification. Source: [[02-markdown-block-syntax]]. Implemented by BIT-EP-0003.

## Requirements

### BIT-SP-0001.R1 — Block start detection and tolerant outline tree building

The parser SHALL treat every line matching `^[ \t]*-([ \t]|$)` that is outside an open code fence (```` ``` ```` / `~~~`) or `#+BEGIN_X … #+END_X` region as a block start, with level = number of indentation characters + 1 (each tab or space counts as 1). It SHALL build the block tree by relative comparison of levels using Logseq's algorithm (`deps/graph-parser/src/logseq/graph_parser/block.cljs:695-768`): a deeper line becomes a child whatever the delta, an outdent attaches to the nearest ancestor with indent ≤ current, and when none matches exactly the block becomes a sibling of the first deeper ancestor. Offsets SHALL be computed in UTF-8 bytes.

#### Scenario: Irregular indentation regression #1902
- GIVEN the file content `"- line1\n    - line2\n      - line3\n     - line4"`
- WHEN the page is parsed
- THEN `line2` is a child of `line1`, `line3` is a child of `line2`, and `line4` is placed exactly as Logseq places it (`extract_test.cljs:81-87`)

#### Scenario: Bullet inside a fence is not a block
- GIVEN a block whose body contains a fence ```` ``` ```` with the line `- x` at column 0 before the closing fence
- WHEN the page is parsed
- THEN `- x` stays in the body of the enclosing block and no new block starts

#### Scenario: Dash without whitespace is not a bullet
- GIVEN the line `-foo`
- WHEN the page is parsed
- THEN it is body text of the current block, not a block start

#### Scenario: Other list markers are body content
- GIVEN lines `* item`, `+ item` and `1. item` at column 0 after `- parent`
- WHEN the page is parsed
- THEN they are body lines of `parent` and create no outline nodes

### BIT-SP-0001.R2 — Top-level ATX heading lines are level-1 heading blocks

The parser SHALL treat a top-level line matching `^#{1,}[ \t]` (no bullet) outside fences as a level-1 heading block with `size` = number of `#`, and bulleted lines indented below it as its children (`block.cljs:568-572`). `#` followed directly by a non-space character SHALL be parsed as a tag, not a heading. A bulleted heading `- ## x` SHALL record the heading size without implying a `heading::` property.

#### Scenario: Bullet-less heading with child
- GIVEN the content `"## hello\n    - world"`
- WHEN the page is parsed
- THEN `## hello` is a level-1 block with heading size 2 and `world` is its child (`extract_test.cljs:26-32`)

#### Scenario: Hash without space is a tag
- GIVEN the top-level line `#foo bar`
- WHEN the page is parsed
- THEN no heading block is created and `foo` is recognized as a tag

#### Scenario: Bulleted heading
- GIVEN the line `- # Title`
- WHEN the block is parsed
- THEN the block has heading size 1, its content keeps `# Title`, and no `heading::` property is reported as written

### BIT-SP-0001.R3 — Pre-block, front matter and page property sources

The parser SHALL treat all bytes before the first block line as the pre-block (the whole file when there is no block line), keeping its raw bytes. Page properties SHALL be parsed from YAML front matter at byte 0 (`---\nkey: value\n---`), from `key:: value` lines and from `#+key: value` lines in the pre-block. `#+key: value` directives found anywhere in the file (including inside blocks) SHALL be hoisted into the page properties, as `collect-page-properties` does (`mldoc.cljc:117-131`). A pre-block whose properties contain `heading` SHALL NOT be treated as a pre-block (`block.cljs:532`).

#### Scenario: Property pre-block
- GIVEN the content `"title:: My Page\nalias:: Mine, [[My page alias]]\ntags:: project, [[multi word]]\n\n- first block"`
- WHEN the page is parsed
- THEN the page properties are `title = "My Page"`, `alias = {"Mine", "My page alias"}`, `tags = {"project", "multi word"}` and `first block` is the first real block

#### Scenario: YAML front matter
- GIVEN a file starting with `---\ntitle: Front\ntags: a\n---\n- block`
- WHEN the page is parsed
- THEN the page title property is `Front` and the front matter bytes are kept verbatim in the pre-block

#### Scenario: Directive deep in the page
- GIVEN the content `"- a\n\t- #+title: x"`
- WHEN the page is parsed
- THEN the page property `title` is `x`

### BIT-SP-0001.R4 — Property line syntax and recognition scope

The parser SHALL recognize a property line with mldoc's rule (`mldoc:lib/syntax/markdown_property.ml`): optional leading whitespace, a key of one or more characters that are neither `:` nor whitespace, then `":: "` followed by the value, or `"::"` at end of line (empty value). Consecutive property lines (and adjacent `#+name: value` lines) SHALL form one property group. Property groups SHALL be recognized anywhere in a block body, with the first group winning, but never inside quotes (`> a:: b`), code fences or `#+BEGIN` blocks. Keys SHALL be normalized for lookup (lower-case, ` ` and `_` → `-`, `custom_id`/`custom-id` → `id`, `block.cljs:204-238`) while the original text is kept.

#### Scenario: Edge keys
- GIVEN body lines `a.b.c:: 1`, `empty::`, `my key:: v`, `key::value`
- WHEN the block is parsed
- THEN `a.b.c = 1` and `empty = ""` are properties, while `my key:: v` and `key::value` are plain text

#### Scenario: Late property group
- GIVEN the block `"- a\n  text\n  late:: prop"`
- WHEN the block is parsed
- THEN the block has property `late = "prop"`

#### Scenario: Properties inside a fence are ignored
- GIVEN a block whose fenced code contains `foo:: bar`
- WHEN the block is parsed
- THEN the block has no property `foo`

### BIT-SP-0001.R5 — Property value semantics

The parser SHALL interpret property values in Logseq's order (`text.cljs:87-187`): keys in the unparsed built-in set (`property.cljs:110-121`) or in `:ignored-page-references-keywords` keep the raw trimmed string; values wrapped in `"…"` are kept verbatim including quotes; otherwise refs (`[[page]]`, `#tag`, `#[[multi word]]`, nested refs) yield a set of page names, skipping macros; only `alias`, `aliases`, `tags` and keys listed in `:property/separated-by-commas` are split on `,` or `，`; `true`/`false` become booleans and `^\d+$` becomes an integer.

#### Scenario: Comma split only for tags/alias
- GIVEN `tags:: foo, bar` and `foo:: a, b`
- WHEN the block is parsed
- THEN `tags = {"foo", "bar"}` and `foo = "a, b"` (string)

#### Scenario: Quoted value suppresses refs
- GIVEN `tags:: "foo, bar"`
- WHEN the block is parsed
- THEN the value is the string `"\"foo, bar\""` and no page refs are produced

#### Scenario: Mixed ref forms
- GIVEN `tags:: a, [[b c]], #d`
- WHEN the block is parsed
- THEN `tags = {"a", "b c", "d"}`

#### Scenario: Typed scalars
- GIVEN `public:: true` and `n:: 1000` and `s:: "1000"`
- WHEN parsed
- THEN `public` is boolean true, `n` is integer 1000, and `s` is the string `"\"1000\""`

### BIT-SP-0001.R6 — Unknown and invalid properties are preserved verbatim

The parser and serializer SHALL preserve unknown and invalid properties verbatim, including their order, original key spelling and original value text. Invalid keys (not valid EDN keywords, containing `"^(){}` or starting with `#`, `property.cljs:22-28`) SHALL be reported as invalid properties, ignored for semantics, and kept in the block text. Property order SHALL be exposed (`properties-order`) and original value text retained (`properties-text-values`).

#### Scenario: Invalid key stays in text
- GIVEN the block `"- a\n  \"x\":: 1\n  (a):: 2"`
- WHEN the page is parsed and saved without edits
- THEN both lines are listed as invalid properties and the file bytes are unchanged

#### Scenario: Order preserved on unrelated edit
- GIVEN a block with properties `zeta:: 1`, `Alpha:: [[X]]`, `mid:: "q"` in that order
- WHEN the user edits only the block's title text
- THEN the three property lines are written back in the same order with the same spelling

### BIT-SP-0001.R7 — Block identity via id:: and on-demand id insertion

The parser SHALL recognize `id:: <uuid>` (also `custom-id`/`custom_id`) as the block identity and SHALL never drop or rewrite a valid `id::` line. Blocks without a valid id SHALL get a session-stable, non-persisted id (ADR-006). When Bitacora creates a block ref or embed to a block without an id, it SHALL insert `id:: <uuid>` into that block after the title line, appended to the existing property group (never into a pre-block). When the same id appears earlier in the same file or in another page, the later block SHALL be flagged as a duplicate and receive a new in-memory id.

#### Scenario: Ref creation writes id
- GIVEN the block `"- Parent block\n  tags:: demo"` without an id
- WHEN the user copies a block ref to it
- THEN the block becomes `"- Parent block\n  tags:: demo\n  id:: 6500c1a4-0000-4000-8000-000000000001"` (the generated uuid) and no other block changes

#### Scenario: Unreferenced blocks stay id-free
- GIVEN a page of 10 blocks without `id::`
- WHEN the page is opened, navigated and closed
- THEN no `id::` line is written

#### Scenario: Duplicate id
- GIVEN two pages containing `id:: 6500c1a4-0000-4000-8000-000000000001`
- WHEN the graph is indexed
- THEN the block in the file loaded first keeps the id and the other one is reported as a duplicate

### BIT-SP-0001.R8 — Reference, tag, embed and macro recognition

The inline scanner SHALL recognize references exactly as Logseq does ([[02-markdown-block-syntax]] §5.1): `[[page]]`, nested `[[a [[b]] c]]` (refs to `a [[b]] c` and `b`), `#tag` ending at whitespace or `, ; . ! ? ' " :`, `#[[multi word]]`, `((uuid))` with a canonical lowercase-hex UUID, labelled refs `[label]([[page]])` and `[label](((uuid)))`, `{{embed [[page]]}}` / `{{embed ((uuid))}}`, macros `{{name args}}` with comma-separated arguments, and refs inside property values. Text inside inline code, fenced code and `#+BEGIN_QUERY` blocks SHALL carry no refs, `\[[x]]` SHALL not be a ref, and `[[assets/x.pdf]]` / `[[draws/x.excalidraw]]` SHALL be treated as links, not pages. Namespace parents of `a/b/c` SHALL be added as refs.

#### Scenario: Tag delimiters
- GIVEN the text `see #tag. and #foo: and #[[a b]]`
- WHEN scanned
- THEN the refs are `tag`, `foo` and `a b`

#### Scenario: Code suppresses refs
- GIVEN the text `` `[[x]]` and \[[y]] and [[z]] ``
- WHEN scanned
- THEN the only ref is `z`

#### Scenario: Embeds and block refs
- GIVEN `{{embed ((6500c1a4-0000-4000-8000-000000000001))}}` and `[see](((6500c1a4-0000-4000-8000-000000000001)))`
- WHEN scanned
- THEN two block refs to `6500c1a4-0000-4000-8000-000000000001` are produced

#### Scenario: Nested tag
- GIVEN `#[[nested [[tag]]]]`
- WHEN scanned
- THEN refs `nested [[tag]]` and `tag` are produced

### BIT-SP-0001.R9 — Task markers and priorities

The parser SHALL recognize a task marker (`TODO DOING DONE LATER NOW WAITING WAIT CANCELED CANCELLED STARTED IN-PROGRESS`) only immediately after the bullet or after the heading hashes and only when followed by a space, matching mldoc 1.5.7 (`mldoc:heading0.ml:16-28`). It SHALL recognize a priority `[#X]` (any single character) right after the marker or bullet. Marker and priority SHALL be exposed as refs (`TODO`, `A`) as `with-page-refs` does.

#### Scenario: Marker requires trailing text
- GIVEN the lines `- LATER` and `- LATER read book`
- WHEN parsed
- THEN the first block has no marker and the second has marker `LATER`

#### Scenario: Marker and priority after heading hashes
- GIVEN `- ## TODO [#A] ship it`
- WHEN parsed
- THEN heading size is 2, marker is `TODO`, priority is `A`

#### Scenario: Not a marker
- GIVEN `- TODOx thing` and `- see TODO later`
- WHEN parsed
- THEN neither block has a marker

### BIT-SP-0001.R10 — SCHEDULED/DEADLINE timestamps and LOGBOOK drawers

The parser SHALL recognize `SCHEDULED: <…>` and `DEADLINE: <…>` lines in a block body, with timestamps `<YYYY-MM-DD Www[ HH:MM][ repeater]>` (active) or `[…]` (inactive) and repeaters `+Nu`, `++Nu`, `.+Nu` with `u ∈ h d w m y`, exposing them as `yyyyMMdd` integers plus a repeated flag. It SHALL recognize drawers `:NAME:` … `:END:` (case-insensitive end) and parse `:LOGBOOK:` CLOCK lines in Logseq's exact format: `CLOCK: [2024-01-01 Mon 10:00:00]` for an open entry and `CLOCK: [start]--[end] =>  HH:MM:SS` (two spaces after `=>`) for a closed one. When Bitacora writes CLOCK lines it SHALL use exactly these formats.

#### Scenario: Repeater
- GIVEN `- TODO task\n  SCHEDULED: <2024-01-01 Mon .+1d>`
- WHEN parsed
- THEN scheduled is `20240101`, repeated is true with kind Dotted, 1 day

#### Scenario: Logbook with open and closed clocks
- GIVEN a `:LOGBOOK:` drawer with `CLOCK: [2024-01-01 Mon 10:00:00]--[2024-01-01 Mon 11:00:00] =>  01:00:00` and `CLOCK: [2024-01-02 Tue 09:00:00]`
- WHEN parsed
- THEN two clock entries are exposed, one closed with duration 01:00:00 and one open

#### Scenario: Inactive timestamp
- GIVEN `DEADLINE: [2024-02-01 Thu]`
- WHEN parsed
- THEN the timestamp is recognized as inactive

### BIT-SP-0001.R11 — Canonical serialization of new and edited blocks

The serializer SHALL write new or edited blocks in Logseq's canonical form ([[02-markdown-block-syntax]] §7, `src/main/frontend/modules/file/core.cljs:34-110`): `unit*(depth-1) + "-" + " " + content` for the first line; continuation lines prefixed `unit*(depth-1) + "  "`, where unit comes from `:export/bullet-indentation` (`:tab` default → `\t`, `:two-spaces`, `:four-spaces`, `:eight-spaces`); an empty block written as bare `-`; blank lines inside a block written as the continuation prefix; no blank lines between blocks; a pre-block written as `trim(content)` followed by exactly one blank line; LF line endings; edge whitespace of each block trimmed.

#### Scenario: Canonical nested block with tab unit
- GIVEN an edited child at depth 2 with content `child with ((6500c1a4-0000-4000-8000-000000000001))\nsecond line of child`
- WHEN serialized with default config
- THEN the bytes are `"\t- child with ((6500c1a4-0000-4000-8000-000000000001))\n\t  second line of child"`

#### Scenario: Two-space unit
- GIVEN `:export/bullet-indentation :two-spaces` and a new block at depth 3 with text `x`
- WHEN serialized
- THEN the line is `"    - x"`

#### Scenario: Empty block and pre-block
- GIVEN a new page with page property `tags:: demo` and one empty block
- WHEN serialized
- THEN the file is `"tags:: demo\n\n-"`

### BIT-SP-0001.R12 — Untouched blocks keep their original bytes

The serializer SHALL NOT rewrite blocks the user did not edit: their original bytes, including spaces vs tabs, blank lines between and inside blocks, trailing whitespace, CRLF line endings, BOM and front matter, SHALL be copied verbatim. For every fixture, `serialize(parse(bytes)) == bytes` SHALL hold, and editing one block SHALL change only that block's byte range (plus `id::` insertion in a referenced block when required).

#### Scenario: Unedited two-space file
- GIVEN a 2-space-indented file with blank lines between blocks and trailing spaces
- WHEN Bitacora loads and saves the page without edits
- THEN the output is byte-identical

#### Scenario: One edited block in a CRLF file
- GIVEN a CRLF file with three blocks
- WHEN the user edits only the second block
- THEN the first and third blocks keep their CRLF bytes and only the second block's lines change

#### Scenario: Tabs and spaces mixture
- GIVEN a file mixing tab- and space-indented children
- WHEN a sibling block elsewhere is edited
- THEN the untouched mixed-indent lines are unchanged

### BIT-SP-0001.R13 — collapsed:: property written on collapse, removed on expand

When the user collapses a block in a way Logseq should see, Bitacora SHALL write `collapsed:: true` into the block's property group, and SHALL remove that line when the block is expanded (`core.cljs:20-32`); a missing value means expanded. Bitacora SHOULD offer a setting that keeps collapse state out of files (in-app state only) so git history stays quiet; with that setting on, existing `collapsed::` lines SHALL still be read and preserved.

#### Scenario: Collapse and expand
- GIVEN the block `"- collapsed parent\n\t- hidden child"`
- WHEN the user collapses `collapsed parent`
- THEN the file becomes `"- collapsed parent\n  collapsed:: true\n\t- hidden child"`, and expanding restores the original bytes

#### Scenario: Collapse state kept out of files
- GIVEN the setting "store collapse state in files" is off
- WHEN the user collapses a block
- THEN the file is not modified

### BIT-SP-0001.R14 — Markdown :PROPERTIES: drawers parsed, converted only on edit

The parser SHALL parse org-style `:PROPERTIES:` … `:END:` drawers inside Markdown blocks as block properties, mapping `id`/`custom_id`/`custom-id` → `id`, `last-modified-at` → `updated-at` and `_` → `-` in keys (`property.cljs:135-158`). Bitacora SHOULD NOT convert the drawer to `key:: value` lines on read; conversion SHALL happen only when the block is edited, which is when Logseq converts it.

#### Scenario: Drawer read without rewrite
- GIVEN `"- a\n  :PROPERTIES:\n  :custom_id: 6500c1a4-0000-4000-8000-000000000001\n  :END:"`
- WHEN the page is parsed and saved without edits
- THEN the block id is `6500c1a4-0000-4000-8000-000000000001` and the file bytes are unchanged

#### Scenario: Drawer converted on edit
- GIVEN the same block
- WHEN the user changes its title to `b`
- THEN it is written as `"- b\n  id:: 6500c1a4-0000-4000-8000-000000000001"`

### BIT-SP-0001.R15 — Minimal property edits and Logseq element placement

When editing a property value, Bitacora SHOULD keep property order and placement and change only the affected line, as Logseq's `insert-property` does (`src/main/frontend/util/property.cljs:226-316`): an existing key is replaced in place; a new key is appended to the end of the existing property group; a block without properties gets the new property after its first line when that line is a title, otherwise at the top. Keys SHALL be written lower-cased as `key:: value` with a trimmed value. New `SCHEDULED:`/`DEADLINE:` lines SHOULD be placed after the title, new properties after the title or SCHEDULED/DEADLINE lines, and a new `:LOGBOOK:` drawer after the properties (`drawer.cljs:53-68`).

#### Scenario: Replace in place
- GIVEN `"- task\n  b:: 1\n  a:: 2"`
- WHEN `b` is set to `3`
- THEN the block becomes `"- task\n  b:: 3\n  a:: 2"`

#### Scenario: Append to group
- GIVEN `"- task\n  b:: 1\n  body text"`
- WHEN property `Status` is set to `open`
- THEN the block becomes `"- task\n  b:: 1\n  status:: open\n  body text"`

#### Scenario: Canonical element order
- GIVEN `- TODO [#A] Parent block #tag` with no other lines
- WHEN the user schedules it for 2024-01-01 with `.+1d`, sets an id and clocks in and out
- THEN the lines are ordered: title, `SCHEDULED: <2024-01-01 Mon .+1d>`, `id:: …`, `:LOGBOOK:` drawer

### BIT-SP-0001.R16 — Content, metadata and identity classification of properties

`bitacora-markdown` SHOULD expose a classification of every property and drawer as Content, Metadata or Identity following [[02-markdown-block-syntax]] §5.4, for use by the block-aware merge (ADR-009): Metadata = `collapsed`, `card-*`, `query-table`/`query-properties`/`query-sort-by`/`query-sort-desc`, `filters`, `hl-*`/`ls-type`, `created-at`/`updated-at`/`last-modified-at` (and `_` variants), marker timestamps `todo`/`doing`/`now`/`later`/`done`, `logseq.query/nlp-date`, `exclude-from-graph-view`, and the `:LOGBOOK:` drawer; Identity = `id`/`custom-id`/`custom_id`; everything else (including user keys, `title`, `alias`, `tags`, `heading`, `background-color`, SCHEDULED/DEADLINE) = Content. The merge policy SHOULD be last-writer-wins per metadata key, union for CLOCK lines, and keep-both/de-duplicate-by-uuid for identity.

#### Scenario: Classify block lines
- GIVEN a block with `collapsed:: true`, `card-repeats:: 2`, `id:: 6500c1a4-0000-4000-8000-000000000001`, `owner:: [[Ana]]` and a `:LOGBOOK:` drawer
- WHEN classified
- THEN `collapsed`, `card-repeats` and the logbook are Metadata, `id` is Identity and `owner` is Content

#### Scenario: Metadata-only diff detection
- GIVEN two versions of a block that differ only by `collapsed:: true` and an extra CLOCK line
- WHEN compared with the classification helper
- THEN the difference is reported as metadata-only

### BIT-SP-0001.R17 — Avoid Logseq serialization quirks when writing

When writing, Bitacora SHOULD avoid Logseq's quirks ([[02-markdown-block-syntax]] §7 consequences 2–3): it SHALL never emit a non-first block without a bullet; it SHOULD never emit a first block with `heading:: true` without a bullet; and it SHOULD avoid writing `key:: value` as the first line of the first bulleted block unless page properties are intended (in which case it writes an un-bulleted pre-block). Files produced this way SHALL re-parse to the same tree in both Bitacora and Logseq.

#### Scenario: heading:: true first block keeps bullet
- GIVEN a page whose first block is `Intro` with `heading:: true`
- WHEN the block is edited and saved
- THEN it is written as `"- Intro\n  heading:: true"` rather than `"Intro\nheading:: true"`

#### Scenario: Property-like first line
- GIVEN the user types `status:: draft` as the only text of the first block
- WHEN it is saved
- THEN Bitacora asks or decides explicitly between a page property (pre-block `status:: draft` + blank line) and a block, and never writes an ambiguous form

### BIT-SP-0001.R18 — Opaque Markdown body content and image metadata preservation

Bitacora SHOULD treat `*`, `+` and `1.` lists, GFM tables, quotes, HTML (`<span>`, `<!-- -->`), hiccup, `#+BEGIN_X` blocks (including `#+BEGIN_QUERY` EDN) and fenced code as opaque Markdown body content of the enclosing block: never segmenting the outline on them and never re-formatting them. It SHOULD preserve image metadata `{:height N, :width M}` after `![](…)` verbatim, and write it in EDN `pr-str` style (`{:height 100, :width 200}`) when the user resizes the image (`editor.cljs:1836-1844`).

#### Scenario: Query block untouched
- GIVEN a block containing `#+BEGIN_QUERY\n{:query [:find (pull ?b [*]) :where [?b :block/refs [[x]]] "q"]}\n#+END_QUERY`
- WHEN a sibling block is edited
- THEN the query block bytes are unchanged and `[[x]]` inside it produces no ref

#### Scenario: Image resize
- GIVEN `![a.png](../assets/a.png){:height 100, :width 200}`
- WHEN the user resizes the image to 300×150
- THEN the text becomes `![a.png](../assets/a.png){:height 150, :width 300}` and nothing else in the block changes

### BIT-SP-0001.R19 — UTF-8 byte offsets, CRLF and BOM tolerance in the parser

The parser SHOULD read input as UTF-8 and compute every span (blocks, properties, refs, timestamps) as byte offsets into the original buffer, as mldoc does (`block.cljs:657`). It SHALL accept `\r\n` as end of line without leaking `\r` into property keys/values, refs or titles, while keeping the `\r` bytes in raw block spans. A leading BOM SHALL be kept in the raw pre-block bytes but excluded from parsed property keys and titles.

#### Scenario: Multi-byte offsets
- GIVEN the file `"- café [[Señor]]\n- b"`
- WHEN parsed
- THEN the second block starts at byte offset 19 and the ref span of `Señor` covers bytes 10..16 exactly

#### Scenario: CRLF property values
- GIVEN `"title:: X\r\n\r\n- a\r\n  k:: v\r\n"`
- WHEN parsed
- THEN the page title is `X`, property `k = "v"` (no `\r`), and re-serializing without edits gives identical bytes
