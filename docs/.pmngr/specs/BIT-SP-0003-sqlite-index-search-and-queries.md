---
id: BIT-SP-0003
type: spec
title: SQLite index, search and queries
status: backlog
author: mcp
labels: [index, search]
created: 2026-10-06T14:21:35Z
updated: 2026-10-07T00:10:13Z
requirements:
  R1:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/location.rs
        - crates/bitacora-index/src/index.rs
        - crates/bitacora-app/src/session.rs#GraphSession
        - crates/bitacora-app/src/views/picker.rs#GraphPicker
      tests:
        - crates/bitacora-index/tests/lifecycle.rs
        - crates/bitacora-app/src/session.rs
        - crates/bitacora-app/src/views/workspace.rs
    verified: {rev: "sha256:584ce8abaaf6c47d", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: claude}
  R2:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/index.rs
        - crates/bitacora-app/src/views/settings/graph_config.rs
      tests:
        - crates/bitacora-index/tests/lifecycle.rs
        - crates/bitacora-app/src/views/settings/tests.rs#changing_the_journal_title_format_asks_to_reindex_before_touching_the_file
    verified: {rev: "sha256:7d22769253fa01d0", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: claude}
  R3:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/schema.rs
        - crates/bitacora-index/src/schema_v1.sql
      tests: [crates/bitacora-index/tests/lifecycle.rs]
    verified: {rev: "sha256:3774303bd6302bc5", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: claude}
  R4:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/parse.rs#parse
        - crates/bitacora-index/src/parsed.rs#ParsedBlock
        - crates/bitacora-index/src/read/outline.rs
      tests:
        - crates/bitacora-index/tests/parse_unit.rs#intervals_are_pre_order
        - crates/bitacora-index/tests/parse_golden.rs
        - crates/bitacora-index/tests/read_outline.rs
    verified: {rev: "sha256:0366541edbd4f1ba", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: claude}
  R5:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/replace.rs
        - crates/bitacora-index/src/writer.rs
      tests: [crates/bitacora-index/tests/writer.rs]
    verified: {rev: "sha256:8888f3866f9fc1ad", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: claude}
  R6:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/reconcile.rs
        - crates/bitacora-app/src/session.rs#open_and_reconcile
        - crates/bitacora-app/src/views/status_bar.rs#AppStatusBar
      tests:
        - crates/bitacora-index/tests/reconcile.rs
        - crates/bitacora-app/src/session.rs
    verified: {rev: "sha256:532f5b458d47c076", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: claude}
  R7:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/reconcile.rs
        - crates/bitacora-index/src/writer.rs
      tests:
        - crates/bitacora-index/tests/reconcile.rs
        - crates/bitacora-index/tests/writer.rs
    verified: {rev: "sha256:1620c5622d5b6951", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: claude}
  R8:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/parse.rs#page_refs_of
        - crates/bitacora-index/src/parsed.rs#PageRefKind
      tests:
        - crates/bitacora-index/tests/parse_unit.rs#ref_kinds_follow_logseq
        - crates/bitacora-index/tests/parse_unit.rs#block_ref_kinds
        - crates/bitacora-index/tests/parse_golden.rs
    verified: {rev: "sha256:535b1afd8f3b95d0", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: claude}
  R9:
    status: backlog
    trace:
      code: [crates/bitacora-index/src/read/refs.rs]
      tests: [crates/bitacora-index/tests/read_refs.rs]
    verified: {rev: "sha256:de18bae1f0c72d39", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: claude}
  R10:
    status: backlog
    trace:
      code: [crates/bitacora-index/src/parse.rs#type_property]
      tests:
        - crates/bitacora-index/tests/parse_unit.rs#typed_properties
        - crates/bitacora-index/tests/parse_unit.rs#comma_separated_keys_from_config
    verified: {rev: "sha256:ef578b62b097ae90", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: claude}
  R11:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/replace.rs
        - crates/bitacora-index/src/read/diagnostics.rs
        - crates/bitacora-index/src/diagnostics.rs
        - crates/bitacora-cli/src/cmd/doctor.rs
      tests:
        - crates/bitacora-index/tests/writer.rs
        - crates/bitacora-index/tests/read_diagnostics.rs
        - crates/bitacora-cli/tests/index_commands.rs
    verified: {rev: "sha256:cdad3517c3101897", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: claude}
  R12:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/carry.rs
        - crates/bitacora-index/src/replace.rs
      tests:
        - crates/bitacora-index/tests/writer.rs
        - crates/bitacora-index/tests/reconcile.rs
    verified: {rev: "sha256:364ad8aee79fb26c", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: claude}
  R13:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/search/query.rs
        - crates/bitacora-index/src/normalize.rs
      tests:
        - crates/bitacora-index/src/search/query.rs
        - crates/bitacora-index/tests/search.rs
    verified: {rev: "sha256:137d1005d6dbf80b", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: claude}
  R14:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/search/mod.rs
        - crates/bitacora-index/src/search/fuzzy.rs
        - crates/bitacora-index/src/search/snippet.rs
        - crates/bitacora-index/src/read/misc.rs#search
        - crates/bitacora-index/src/pool.rs
        - crates/bitacora-app/src/views/palette.rs
      tests:
        - crates/bitacora-index/tests/search.rs
        - crates/bitacora-index/tests/search.rs#cached_fuzzy_titles_follow_writes
        - crates/bitacora-index/tests/bench_search.rs#search_latency_smoke_on_a_1000_page_graph
        - crates/bitacora-index/tests/bench_search.rs#search_latency_on_a_5000_page_graph
        - crates/bitacora-index/tests/bench_large.rs
        - crates/bitacora-index/src/search/snippet.rs
        - crates/bitacora-index/tests/read_misc.rs#backlink_counts_search_and_mentions_back_the_app_views
        - crates/bitacora-app/src/views/palette.rs#run_search_ranks_the_exact_page_first_and_honours_scopes
        - crates/bitacora-app/src/views/palette.rs#typing_searches_and_enter_opens_the_selected_result
    verified: {rev: "sha256:0de1d55d8b600c57", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: mcp}
  R15:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/search/mod.rs#set_substring
        - crates/bitacora-index/src/writer.rs#set_substring
        - crates/bitacora-runtime/src/live.rs#set_substring
        - crates/bitacora-app/src/session.rs#run
        - crates/bitacora-app/src/views/settings/mod.rs#request_substring
      tests:
        - crates/bitacora-index/tests/search.rs
        - crates/bitacora-app/src/views/settings/tests.rs#search_substring_asks_first_and_persists_across_a_restart
        - crates/bitacora-app/src/views/settings/tests.rs#the_session_applies_search_substring_and_mcp_settings_from_the_app_settings
    verified: {rev: "sha256:4e7222feba77143b", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: claude}
  R16:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/writer.rs
        - crates/bitacora-index/src/reconcile.rs
        - crates/bitacora-index/src/dump.rs
        - crates/bitacora-cli/src/cmd/reindex.rs
      tests:
        - crates/bitacora-index/tests/property.rs
        - crates/bitacora-index/tests/reconcile.rs
        - crates/bitacora-index/tests/bench_cold_build.rs
        - crates/bitacora-index/tests/bench_cold_build.rs#cold_build_smoke_is_fast_and_consistent
        - crates/bitacora-index/tests/bench_large.rs
        - crates/bitacora-cli/tests/index_commands.rs
    verified: {rev: "sha256:dcf89c2b25059c3d", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: mcp}
  R17:
    status: backlog
    trace:
      code: [crates/bitacora-index/src/read/unlinked.rs]
      tests: [crates/bitacora-index/tests/read_refs.rs]
    verified: {rev: "sha256:06b872e03ff0f63d", commit: b12ad4e73162bfd64905c054c5a1e0d13ae4916e, at: 2026-10-06T22:36:02Z, by: claude}
  R18:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/query/dsl.rs
        - crates/bitacora-index/src/query/compile.rs
        - crates/bitacora-index/src/query/dates.rs
        - crates/bitacora-index/src/query/mod.rs
        - crates/bitacora-app/src/render/query/mod.rs
        - crates/bitacora-app/src/render/query/table.rs
        - crates/bitacora-app/src/views/widgets/query_block.rs
      tests:
        - crates/bitacora-index/tests/query_simple.rs
        - crates/bitacora-app/src/views/widgets/tests.rs
    verified: {rev: "sha256:e662d812be0472fc", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
  R19:
    status: backlog
    trace:
      code:
        - crates/bitacora-index/src/query/advanced.rs
        - crates/bitacora-index/src/query/datalog.rs
        - crates/bitacora-index/src/query/edn.rs
        - crates/bitacora-app/src/render/query/mod.rs
        - crates/bitacora-app/src/views/widgets/query_block.rs
      tests:
        - crates/bitacora-index/tests/query_advanced.rs
        - crates/bitacora-app/src/views/widgets/tests.rs
    verified: {rev: "sha256:72f80f0b36a5ef1f", commit: a7b6b731dc4613962d70c4e9e9ac30c002f2f0d1, at: 2026-10-06T21:59:44Z, by: claude}
---

## Purpose
A rebuildable SQLite cache gives instant search, backlinks, journals, tasks and query results without ever being the source of truth.

## Scope
Schema and migrations, incremental indexing pipeline, path-refs and backlinks, FTS5 search and ranking, simple query DSL and Datalog subset. Source: [[sqlite-index-schema]], [[03-parsing-indexing-search]]. Implemented by BIT-EP-0005 and BIT-EP-0013.

## Requirements

### BIT-SP-0003.R1 — Index is a disposable per-graph cache stored outside the graph

The system SHALL store the index of each graph in a single SQLite file at `<data_dir>/bitacora/graphs/<graph-id>/index.sqlite`, where `graph-id = hex(blake3(canonical absolute graph path))[..16]` (ADR-005), and SHALL never create or modify any file inside the graph directory for indexing purposes. The index SHALL be treated as a cache: deleting it at any time SHALL cause an automatic full rebuild on next open with no user data loss.

#### Scenario: Index location is outside the graph
- GIVEN a graph at `/home/ana/notes`
- WHEN Bitacora opens the graph for the first time
- THEN `index.sqlite` is created under the platform data dir (e.g. `~/.local/share/bitacora/graphs/<16-hex>/index.sqlite` on Linux)
- AND `git status` in `/home/ana/notes` shows no new or modified files

#### Scenario: Deleted index is rebuilt
- GIVEN an indexed graph whose `index.sqlite` was deleted while the app was closed
- WHEN the graph is opened
- THEN a full rebuild runs and page, block and search results equal those before deletion (block UUIDs from `id::` identical)

### BIT-SP-0003.R2 — Rebuild on schema, parser or config change and on corruption

The system SHALL keep `schema_version` in `PRAGMA user_version` and `parser_version`, `config_hash` and `normalizer_version` in the `meta` table. On open it SHALL delete and rebuild the database when `user_version` differs from the compiled schema version, when SQLite reports `SQLITE_CORRUPT` or `PRAGMA quick_check` fails; it SHALL reparse every file when `parser_version` or the hash of the index-relevant `config.edn` keys (`:journal/page-title-format`, `:file/name-format`, `:journals-directory`, `:pages-directory`, `:hidden`, `:property/separated-by-commas`, `:property-pages/enabled?`, `:property-pages/excludelist`, `:ignored-page-references-keywords`, `:preferred-format`) changes; and it SHALL only recompute `search_text` and rebuild FTS when `normalizer_version` changes.

#### Scenario: Schema version bump
- GIVEN an index with `user_version = 1` and a binary built with schema version 2
- WHEN the graph is opened
- THEN the old file is deleted, a new schema v2 database is created and fully built

#### Scenario: Relevant config change
- GIVEN an indexed graph with `:journal/page-title-format "MMM do, yyyy"`
- WHEN `logseq/config.edn` changes it to `"yyyy-MM-dd"`
- THEN `meta.config_hash` no longer matches and every file is reparsed so journal pages get the new names

#### Scenario: Irrelevant config change
- GIVEN an indexed graph
- WHEN only `:ui/show-brackets?` changes in `config.edn`
- THEN no file is reparsed

### BIT-SP-0003.R3 — Schema v1 tables, FTS tables and derived views

The system SHALL create the schema defined in [[sqlite-index-schema]] §3: tables `meta`, `files`, `pages`, `page_aliases`, `page_tags`, `blocks`, `block_page_refs`, `block_block_refs`, `block_properties`, `block_property_values`, `diagnostics`; FTS5 tables `blocks_fts` (`unicode61 remove_diacritics 2`, prefix 2 3), `blocks_fts_tri` (`trigram`) and `pages_fts` (`trigram`) as external-content tables maintained by triggers; and the views `block_path_refs`, `page_properties`, `page_property_values` and `tasks`. There is no `file_snapshots` table: the merge base for external edits is kept in memory only (ADR-017). Connections SHALL be opened with WAL, `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout=5000` and a bundled SQLite >= 3.45.

#### Scenario: Fresh schema
- GIVEN an empty data directory
- WHEN the index is opened
- THEN `sqlite_master` lists all tables, FTS tables, triggers and views above and `PRAGMA journal_mode` returns `wal`

#### Scenario: FTS stays in sync through triggers
- GIVEN a block row with `search_text = 'meeting notes'`
- WHEN the block row is deleted
- THEN `SELECT rowid FROM blocks_fts WHERE blocks_fts MATCH 'meeting'` returns no row for it

### BIT-SP-0003.R4 — Outline stored as pre-order intervals per file

The system SHALL store each file's outline as pre-order `ord`, `subtree_end`, `depth`, `sibling_idx` and `parent_id` (pre-block `ord = 0`), and SHALL answer subtree, ancestor (breadcrumb) and page-outline queries with interval predicates on `(file_id, ord)` rather than recursive traversal. Page outline reads SHALL support skipping collapsed subtrees and pagination.

#### Scenario: Subtree query
- GIVEN a page `- A` / `  - B` / `    - C` / `- D`
- WHEN the subtree of block A is requested
- THEN A, B and C are returned in order and D is not, using `ord BETWEEN A.ord AND A.subtree_end`

#### Scenario: Breadcrumb
- GIVEN the same page
- WHEN the ancestors of C are requested
- THEN A and B are returned in that order

#### Scenario: Collapsed subtree is skipped
- GIVEN block A has `collapsed:: true`
- WHEN the visible outline of the page is requested
- THEN A and D are returned and B and C are not

### BIT-SP-0003.R5 — Transactional per-file replace with page upsert and placeholder GC

The system SHALL replace all rows derived from one file (blocks, refs, properties, aliases, tags, diagnostics) in a single `BEGIN IMMEDIATE` transaction; SHALL upsert pages by normalized name (`page-name-sanity-lc`) so `pages.id` stays stable; SHALL demote a page whose defining file disappeared or no longer defines it to a placeholder (`file_id = NULL`); and SHALL garbage-collect placeholder pages that nothing references, never deleting built-in pages (`TODO`, `DONE`, `A`, `B`, `C`, `Contents`, `Favorites`, `card`). Readers SHALL never observe a partially replaced file. No file content snapshot is stored in the index (ADR-017).

#### Scenario: Placeholder GC
- GIVEN `pages/a.md` is the only file containing `[[Zeta]]` and no `zeta.md` exists
- WHEN the ref is removed from `a.md` and the file is reindexed
- THEN the placeholder page `zeta` is deleted from `pages`

#### Scenario: Deleted file keeps referenced page as placeholder
- GIVEN `pages/beta.md` exists and `pages/a.md` contains `[[Beta]]`
- WHEN `beta.md` is deleted
- THEN page `beta` remains with `file_id IS NULL` and still appears in a's refs

#### Scenario: Built-ins survive
- GIVEN no block in the graph has a `TODO` marker
- WHEN GC runs
- THEN page `todo` still exists with `is_builtin = 1`

### BIT-SP-0003.R6 — Startup reconcile with size/mtime prefilter and blake3 authority

The system SHALL reconcile the index against disk at startup and on watcher overflow: walk the graph applying Logseq's ignore rules (dot-paths, `logseq/bak`, `logseq/.recycle`, `logseq/version-files`, `node_modules`, `graphs-txid.edn`, `pages-metadata.edn`) and `:hidden`; delete index rows for files gone from disk; skip files whose `(size, mtime_ns)` match; hash others with blake3 and only touch metadata when the hash equals `files.content_hash`; and enqueue a reparse otherwise. Parse jobs SHALL be prioritised: today's journal and home page, UI-requested pages, journals newest first, then pages. A `paranoid_scan` setting SHOULD hash every file.

#### Scenario: Unchanged graph
- GIVEN an index fully up to date with a 5,000-file graph
- WHEN the graph is reopened
- THEN no file is parsed and no file content is read

#### Scenario: Touched but identical file
- GIVEN `pages/x.md` whose mtime changed via `git checkout` but whose bytes are unchanged
- WHEN reconcile runs
- THEN only `size`/`mtime_ns` are updated and blocks keep their rowids

#### Scenario: Ignored paths
- GIVEN files `logseq/bak/pages/x/2024-01-01T10_00_00.000Z.Desktop.md` and `.git/HEAD`
- WHEN reconcile runs
- THEN neither appears in `files`

### BIT-SP-0003.R7 — Parallel pure parsing and a single writer thread

The system SHALL parse files on a worker pool with a pure function `parse(path, bytes, config) -> ParsedFile` that touches no database, SHALL serialize all index writes through one writer thread owning the only write connection, and SHALL serve UI, MCP and search reads from a pool of read-only WAL connections. After each committed file replace the writer SHALL emit `IndexEvent::FileReplaced { file_id, page_ids_touched, block_uuids_added, block_uuids_removed }`.

#### Scenario: Reads during a cold build
- GIVEN a cold build of a 50k-block graph in progress
- WHEN the UI searches for `meeting`
- THEN the query returns results from already committed files without waiting for the build to finish

#### Scenario: Change notification
- GIVEN the UI shows page `Projects`
- WHEN `pages/Projects.md` is reindexed
- THEN an `IndexEvent::FileReplaced` naming the page id of `projects` is emitted after commit

### BIT-SP-0003.R8 — Logseq-compatible page and block references

The system SHALL compute block refs exactly like Logseq 0.10.x and store them in `block_page_refs` with a `kind`: `[[x]]`, `#x`, `#[[x]]` and nested links (1/2), ref-valued property values (3), property names unless `:property-pages/enabled? false` or excluded (4), task marker (5), priority (6), namespace parents of referenced pages (7) and `{{embed [[x]]}}` (8). Block refs `((uuid))`, `{{embed ((uuid))}}` and `[label](((uuid)))` SHALL be stored in `block_block_refs` by target UUID without a foreign key, so dangling refs are allowed.

#### Scenario: Marker, priority and namespace refs
- GIVEN a block `TODO [#A] review [[work/q3]]`
- WHEN it is indexed
- THEN `block_page_refs` contains pages `todo` (kind 5), `a` (kind 6), `work/q3` (kind 1) and `work` (kind 7)

#### Scenario: Property refs
- GIVEN a block with `type:: [[book]], #fiction`
- WHEN it is indexed
- THEN it references `type` (kind 4), `book` and `fiction` (kind 3)

#### Scenario: Dangling block ref
- GIVEN a block `see ((6650a1b2-0000-4000-8000-000000000001))` whose target does not exist
- WHEN it is indexed
- THEN a `block_block_refs` row with that `target_uuid` exists and resolving it returns nothing

### BIT-SP-0003.R9 — Path-refs, alias closure and linked references

The system SHALL derive path-refs as `{page} ∪ own refs ∪ ancestors' refs` (pages only) via the interval join view `block_path_refs`, without a materialized table unless benchmarks require one. Linked references of page P SHALL return blocks whose path-refs intersect the symmetric 2-hop alias closure of P, excluding blocks on P itself, ordered by page then outline order, and SHALL honour P's `filters::` include/exclude page property.

#### Scenario: Inherited ref
- GIVEN on page `Log`: `- met [[Ana]]` / `  - discussed budget`
- WHEN linked references of `Ana` are requested
- THEN the parent block is returned and the child is reachable as part of its subtree (path-refs of the child include `ana`)

#### Scenario: Alias closure
- GIVEN page `Ana Lopez` with `alias:: Ana` and a block on `Log` containing `[[Ana]]`
- WHEN linked references of `Ana Lopez` are requested
- THEN that block is included

#### Scenario: Own page excluded
- GIVEN page `Ana` contains a block `[[Ana]] self note`
- WHEN linked references of `Ana` are requested
- THEN that block is not included

### BIT-SP-0003.R10 — Property values parsed with Logseq rules into an EAV store

The system SHALL store block and page properties in `block_properties` (normalized key, position, raw key, raw value, value type, builtin flag) and `block_property_values` (one row per scalar or ref-set element with `value_norm`, `value_num`, `ref_page_id`), parsing values with Logseq's rules: refs set for `[[x]]`/`#x` and for comma-separated keys from `:property/separated-by-commas`, integer, boolean, quoted string verbatim, otherwise string. Page properties SHALL be the properties of the page's pre-block.

#### Scenario: Integer and boolean values
- GIVEN a block with `rating:: 4` and `public:: true`
- WHEN it is indexed
- THEN `rating` has `value_type = 1, value_num = 4` and `public` has `value_type = 2, value_num = 1`

#### Scenario: Comma-separated refs
- GIVEN `:property/separated-by-commas #{:authors}` and a block `authors:: Ana, Ben`
- WHEN it is indexed
- THEN `block_property_values` has rows `ana` and `ben` with `ref_page_id` set

#### Scenario: Raw value kept
- GIVEN `title:: "Hello, World"`
- WHEN it is indexed
- THEN `raw_value` equals `"Hello, World"` byte for byte

### BIT-SP-0003.R11 — Diagnostics for duplicates, invalid properties and parse errors

The system SHALL record in `diagnostics` (with file, kind, severity, line and message) duplicate page titles across files (`duplicate_page`, first file owns the page), duplicate `id::` values across files (`duplicate_block_id`, newcomer gets a fresh UUID), invalid property keys, case-only path conflicts, oversized blocks or files (`too_large`) and parse errors, and SHALL never rewrite a user file to fix them. Diagnostics of a file SHALL be replaced together with the file.

#### Scenario: Duplicate page title
- GIVEN `pages/foo.md` and `pages/other.md` with `title:: Foo`
- WHEN both are indexed
- THEN page `foo` is owned by the first indexed file, the second file has `status = 'duplicate_page'` and a `duplicate_page` diagnostic

#### Scenario: Duplicate block id
- GIVEN two blocks in different files with the same `id:: 6650a1b2-0000-4000-8000-000000000001`
- WHEN both are indexed
- THEN only one keeps that UUID, the other gets a fresh UUID and a `duplicate_block_id` diagnostic, and neither file changes on disk

### BIT-SP-0003.R12 — Block UUID carry-over across reparses

The system SHOULD keep block UUIDs stable across reparses of a file when `id::` is absent, assigning UUIDs with precedence explicit `id::` (`uuid_source = 1`) > carried over by a Myers/patience diff of `(depth, content_hash)` sequences with positional pairing at similarity >= 0.5 (`uuid_source = 2`) > fresh UUIDv7 (`uuid_source = 0`), and SHALL expose `uuid_source` so the editor writes `id::` before a block is referenced. File renames detected by equal hash SHOULD carry UUIDs from the old path. The diff baseline is the file's previous `blocks` rows (including their `content`), read before the per-file replace; no `file_snapshots` table is kept (ADR-017).

#### Scenario: Edit elsewhere keeps UUID
- GIVEN a page with blocks A, B, C without `id::` and B's index UUID is U
- WHEN an external editor changes A's text and the file is reindexed
- THEN B still has UUID U with `uuid_source = 2`

#### Scenario: Explicit id wins
- GIVEN a block with `id:: 6650a1b2-0000-4000-8000-000000000001`
- WHEN the file is reparsed after any edit
- THEN the block's UUID is that value with `uuid_source = 1`

#### Scenario: Rename by hash
- GIVEN `pages/a.md` is renamed to `pages/b.md` with identical bytes
- WHEN the watcher reports the rename
- THEN `files.path` is updated and all block UUIDs are unchanged

### BIT-SP-0003.R13 — Search text normalization

The system SHALL derive `blocks.search_text` and `pages.search_title` with `normalize(s)` = remove hidden built-in property lines (`id::`, `collapsed::`, `heading::`, `created-at::`, ...) → NFKC → lower-case → strip diacritics (default on, configurable), and SHALL apply the same function to user queries. Blocks longer than `search.max_block_len` (default 10,000 chars) SHALL be truncated for indexing and flagged with a `too_large` diagnostic, not dropped.

#### Scenario: Accent and case folding
- GIVEN a block `Reunión con Álvaro`
- WHEN the user searches `reunion alvaro`
- THEN the block is found

#### Scenario: Hidden properties not searchable
- GIVEN a block `call mom` with `id:: 6650a1b2-0000-4000-8000-000000000001`
- WHEN the user searches `6650a1b2`
- THEN that block is not returned by block search

#### Scenario: Long block truncated
- GIVEN a block of 25,000 characters
- WHEN it is indexed
- THEN its first 10,000 characters are searchable and a `too_large` diagnostic exists

### BIT-SP-0003.R14 — Ranked hybrid search pipeline with snippets and scopes

The system SHALL answer a search query by: parsing ` and `/`&`, ` or `/`|`, ` not ` into FTS5 operators and double-quoting every other token; ranking exact page title/alias matches first; running page title trigram search plus an in-memory subsequence fuzzy pass (`nucleo-matcher`); running block word search ranked by `bm25(blocks_fts)`; running trigram substring search (or `LIKE` for queries under 3 characters); merging with Reciprocal Rank Fusion (k = 60) deduplicated by id; and building highlighted snippets in Rust from `content`. It SHALL support scopes: current page, journals only, pages only. Search SHALL return within 50 ms p95 on a 5,000-page fixture graph.

#### Scenario: Exact title first
- GIVEN page `Rust` and 200 blocks containing `rust`
- WHEN the user searches `rust`
- THEN the first result is the page `Rust`

#### Scenario: FTS syntax neutralized
- GIVEN a block `C++ "templates" (advanced)`
- WHEN the user searches `c++ (advanced`
- THEN no FTS syntax error occurs and the block is returned

#### Scenario: Short query fallback
- GIVEN a block `go to 東京`
- WHEN the user searches `東京`
- THEN the block is returned via the `LIKE` fallback

#### Scenario: Performance
- GIVEN the generated 5,000-page benchmark graph
- WHEN 1,000 random 1-3 word queries are run
- THEN p95 latency is below 50 ms

### BIT-SP-0003.R15 — Optional trigram block index for substring and CJK search

The system SHOULD maintain `blocks_fts_tri` (FTS5 `trigram`) for substring and CJK search, and SHALL provide a setting `search.substring = false` that drops the trigram block index (and its triggers) to reduce database size, in which case substring search falls back to `LIKE` over `search_text`.

#### Scenario: Substring match
- GIVEN a block `unbelievable`
- WHEN the user searches `believ`
- THEN the block is returned via `blocks_fts_tri`

#### Scenario: Trigram disabled
- GIVEN `search.substring = false`
- WHEN the index is opened
- THEN `blocks_fts_tri` does not exist and searching `believ` still returns the block through the `LIKE` fallback

### BIT-SP-0003.R16 — Cold build fast path and rebuild equals incremental

The system SHOULD build an empty index with FTS triggers dropped and `foreign_keys=OFF`, inserting about 200 files per transaction, then run FTS `'rebuild'` for all FTS tables, recreate triggers, run `PRAGMA foreign_key_check` and `ANALYZE`. The result of a full rebuild SHALL equal the result of incremental updates for the same final graph state, ignoring rowids and non-`id::` block UUIDs. A cold build SHOULD complete in under 5 s for a 50k-block graph.

#### Scenario: Rebuild equals incremental
- GIVEN fixture graph G indexed incrementally through a scripted sequence of 50 file edits, renames and deletions
- WHEN a second index is built from scratch over the final state of G
- THEN a canonical dump of both (pages, blocks by `(path, ord)`, refs, properties, aliases, tags, diagnostics, FTS hits) is identical

#### Scenario: Cold build time
- GIVEN the generated 50k-block benchmark graph
- WHEN a cold build runs on the reference CI machine
- THEN it completes in under 5 s and `PRAGMA foreign_key_check` returns no rows

### BIT-SP-0003.R17 — Unlinked references via FTS prefilter and Logseq regex

The system SHOULD compute unlinked references of page P by an FTS phrase prefilter over P's name and aliases (`"name" OR "alias"`) excluding blocks on P, followed by Logseq's regex `(?i)(^|[^\[#0-9a-zA-Z]|((^|[^\[])\[))NAME($|[^0-9a-zA-Z])` on content with the LOGBOOK drawer removed, and SHALL NOT perform a full scan of all blocks.

#### Scenario: Plain mention
- GIVEN page `Ana` and a block `call Ana tomorrow`
- WHEN unlinked references of `Ana` are requested
- THEN the block is returned

#### Scenario: Linked mention excluded
- GIVEN a block `call [[Ana]] tomorrow`
- WHEN unlinked references of `Ana` are requested
- THEN the block is not returned

#### Scenario: Word boundary
- GIVEN a block `Anaconda setup`
- WHEN unlinked references of `Ana` are requested
- THEN the block is not returned

### BIT-SP-0003.R18 — Simple query DSL compiled to SQL with Logseq semantics

The system SHALL compile the complete Logseq simple query DSL (`{{query ...}}`) to SQL: `[[x]]`/`#x` via path-refs (no alias expansion), `"text"`, `task`, `priority`, `property` (1 and 2 args), `page-property`, `between` (journal day and timestamp property forms), `page`, `namespace`, `page-tags`, `all-page-tags`, `sort-by`, `sample`, `and`/`or`/`not`. It SHALL apply Logseq's result-type rule (blocks if any of page-ref, text, between, property, task, priority or page appears outside page-level leaves; otherwise pages), NULL-safe negation, date keywords (`today`, `yesterday`, `tomorrow`, `±N(d|w|m|y)`, `[[Journal title]]`, `now`, `±Nh`, `±Nmin`), and exclude the block containing the query. Deviations (case-insensitive text and property values) SHALL be documented in [[sqlite-index-schema]] §7.4.

#### Scenario: Tasks of a project
- GIVEN blocks `TODO write [[project-x]] spec` and `DONE ship [[project-x]]`
- WHEN `{{query (and [[project-x]] (task TODO DOING))}}` is evaluated
- THEN only the TODO block is returned (block result type)

#### Scenario: Negation keeps non-tasks
- GIVEN journal blocks `standup notes` and `DONE standup prep` within the last 7 days
- WHEN `{{query (and (between -7d today) (not (task DONE)) "standup")}}` is evaluated
- THEN `standup notes` is returned and `DONE standup prep` is not

#### Scenario: Page result type
- GIVEN page `Dune` with `type:: book` and `tags:: fiction`
- WHEN `{{query (and (page-property type book) (page-tags fiction))}}` is evaluated
- THEN the result is the page list `[Dune]`

### BIT-SP-0003.R19 — Advanced Datalog query subset compiled to SQL

The system SHOULD compile the documented Datalog subset of [[sqlite-index-schema]] §8 (`:find`/`:in`/`:where` over the listed `:block/*` attributes, predicates `=`, `not=`, comparisons, `contains?`, `get-else`, `missing?`, string predicates, `re-find`; `not`, `not-join`, `or`, `or-join`; Logseq `:inputs` such as `:current-page`, `:today`, `:-7d`; find specs `?x`, `[?x ...]`, `(pull ?b [*])`, `count`/`min`/`max`) to SQL, and SHALL report unsupported constructs (`:result-transform`, `:view`, unknown rules or functions) with a clear "unsupported" message naming the construct, still showing raw results when possible.

#### Scenario: Supported query
- GIVEN `{:query [:find (pull ?b [*]) :where [?b :block/marker ?m] [(contains? #{"NOW" "DOING"} ?m)]]}`
- WHEN it is evaluated
- THEN all blocks with marker NOW or DOING are returned

#### Scenario: Unsupported view
- GIVEN an advanced query with `:view (fn [rows] [:div ...])`
- WHEN it is evaluated
- THEN the result shows the message `unsupported: :view` and the raw rows
