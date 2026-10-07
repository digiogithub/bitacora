# SQLite index schema for Bitacora

> Status: proposal (v1). Based on the analysis in [[03-parsing-indexing-search]] of Logseq `0.10.15` (commit `03bcefbd`) and the DB-graph search on Logseq `master` (commit `22a29b30`).
> Logseq references are `path:LINE` relative to the Logseq repo root. `master:` refers to the `master` branch.
> Related: [[01-file-graph-layout]], [[02-markdown-block-syntax]], [[04-editor-outliner-operations]].

## Summary

Bitacora keeps one SQLite database per graph as a **disposable index** over the Markdown files. The files are the only source of truth. The index can be deleted at any moment and rebuilt from disk with the same result, apart from block UUIDs that are not written in the files (§2.3). It holds:

- **Metadata:** `files`, `pages`, `blocks` (an outline tree stored as pre-order `ord` + `subtree_end` intervals), `block_page_refs`, `block_block_refs`, `block_properties` / `block_property_values` (EAV), `page_aliases`, `page_tags`, task columns with a `tasks` view, and `diagnostics`.
- **Search:** two external-content FTS5 tables over the blocks (`unicode61` for ranked word search and `trigram` for substring and CJK search), plus a `trigram` FTS5 table over page titles and aliases.
- **Bookkeeping:** `meta` (schema version, parser version, config hash) and per-file `(size, mtime_ns, blake3)` for incremental reindexing.

The indexing pipeline: **watcher/scan → stat filter → hash diff → parse (worker pool) → one writer thread → per-file transaction "delete the file's blocks, insert the new ones, upsert pages, GC placeholder pages"**. Path-refs (Logseq's inherited refs) are not stored. They are computed with an interval join (`descendants = same file AND ord BETWEEN a.ord AND a.subtree_end`), which is cheap because a block's ancestors always live in its own file.

The simple-query DSL compiles to a boolean expression of `id IN (subquery)` predicates. Each DSL filter maps to one indexed SQL fragment (§7). A documented subset of advanced Datalog compiles to SQL joins (§8).

---

## 1. Principles

1. **Files win.** On any disagreement between the index and the disk, the disk is right and the index is updated. The index never writes Markdown. Writes come from the editor layer ([[04-editor-outliner-operations]]), which then feeds the same pipeline.
2. **Rebuildable.** No migrations. When `schema_version` changes, or the DB is corrupt (`SQLITE_CORRUPT`, failed `PRAGMA quick_check`), the DB file is deleted and rebuilt. Logseq does the same for its search DB (`src/electron/electron/search.cljs:34-44`). When `parser_version` or `config_hash` changes, every file is reparsed.
3. **File-local recomputation.** Everything derived from a file (blocks, refs, properties, path-refs, the page's aliases and tags) is replaced as one unit per file. Global state (pages) is upserted, and pages are garbage-collected when nothing points to them.
4. **Logseq-compatible semantics.** Names, refs, path-refs and query semantics follow Logseq exactly. Any deviation is listed in §7.4.
5. **One writer, many readers.** WAL mode. A single writer thread owns the write connection. The UI and search read through a pool of read-only connections.

### 1.1 Location and connection settings

- Default location: `<app-data>/bitacora/graphs/<graph-id>/index.sqlite`, where `graph-id = hex(blake3(canonical absolute graph path))[..16]`. The file lives **outside** the graph so git and sync tools never see it. Logseq also ignores dot-dirs (`deps/common/src/logseq/common/graph.cljs:63-64`), so `.bitacora/` inside the graph would be safe for Logseq, but it would show up in git (see Open questions).
- Pragmas on open:
  ```sql
  PRAGMA journal_mode = WAL;
  PRAGMA synchronous = NORMAL;      -- it's a cache; durability of the last tx is not critical
  PRAGMA foreign_keys = ON;         -- OFF during cold build for speed
  PRAGMA temp_store = MEMORY;
  PRAGMA mmap_size = 268435456;     -- 256 MiB
  PRAGMA cache_size = -65536;       -- 64 MiB
  PRAGMA busy_timeout = 5000;
  ```
- SQLite ≥ 3.45, bundled through `rusqlite` with the `bundled` feature. This gives the FTS5 `trigram` tokenizer (≥ 3.34) and `remove_diacritics` for trigram (≥ 3.45).

---

## 2. Identity

### 2.1 Files and pages

- `files.path` is the path relative to the graph root, with `/` separators and **NFC**-normalized, as in Logseq's `path-normalize` (`deps/graph-parser/src/logseq/graph_parser/util.cljs:24-28`).
- `pages.name` is the unique key, computed exactly as `page-name-sanity-lc`: `lower-case` → strip leading/trailing `/` → NFC (`util.cljs:134-165`).
- `pages.id` is a stable integer while the DB lives. Pages are upserted by `name` and never deleted during a file replace.
- `pages.uuid = UUIDv5(NS_BITACORA_PAGE, name)`, so it is deterministic across rebuilds. Logseq uses random squuids for pages; the Logseq page UUID only matters for Logseq's own transit cache.

### 2.2 Blocks

- `blocks.id` is an internal rowid. It is **reassigned on every reparse** of the file. The UI and APIs must address blocks by `uuid`.
- `blocks.uuid` is assigned with this precedence:
  1. `id::` property (also `custom_id` / `custom-id`), if it is a valid UUID (`deps/graph-parser/src/logseq/graph_parser/block.cljs:424-432`). `uuid_source = 1`.
  2. Carried over from the previous index state of the same file by diff (§2.3). `uuid_source = 2`.
  3. A fresh UUIDv7. `uuid_source = 0`.
- **Duplicates:** if an explicit `id::` already belongs to a block in **another** file, the newcomer gets a fresh UUID and a `diagnostics` row (`duplicate_block_id`), as in Logseq `fix-block-id-if-duplicated!` (`block.cljs:635-644`). The index never rewrites the file.

### 2.3 UUID carry-over (2-way diff)

This is Logseq's `diff-merge-uuids-2ways` (`src/main/frontend/handler/common/file.cljs:57-68`, `src/main/frontend/fs/diff_merge.cljs:27-92`), reimplemented:

1. Before deleting the file's blocks, the writer loads `(ord, depth, content_hash, uuid, uuid_source)` for the file.
2. It runs a Myers or patience diff over the sequences `(depth, content_hash)` of the old and new blocks.
3. Equal blocks inherit the old UUID.
4. For each replaced hunk, old and new blocks are paired positionally when they have the same depth and a normalized edit similarity of at least 0.5. Paired blocks inherit the old UUID.
5. Explicit `id::` always wins, and no UUID is used twice.

**Caveat:** carried-over UUIDs exist only in the index. A full rebuild assigns new ones. Anything that must survive a rebuild (cross-file `((uuid))` refs, bookmarks, links shared outside the app) must have its UUID written into the file as `id::`. Logseq makes the same compromise with `set-missing-block-ids!` (`src/main/frontend/fs/watcher_handler.cljs:28-42`).

---

## 3. DDL

```sql
-- =====================================================================
-- Bitacora index schema v1
-- =====================================================================
PRAGMA user_version = 1;           -- == schema_version; mismatch => delete & rebuild

CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) WITHOUT ROWID;
-- keys: schema_version, parser_version, config_hash, normalizer_version,
--       graph_root, created_at, last_full_scan_at, sqlite_version

-- ---------------------------------------------------------------------
-- Files
-- ---------------------------------------------------------------------
CREATE TABLE files (
  id             INTEGER PRIMARY KEY,
  path           TEXT    NOT NULL UNIQUE,   -- relative, '/', NFC
  kind           TEXT    NOT NULL CHECK (kind IN
                   ('page','journal','whiteboard','config','css','other')),
  format         TEXT    CHECK (format IN ('markdown','org','edn') OR format IS NULL),
  size           INTEGER NOT NULL,
  mtime_ns       INTEGER NOT NULL,
  content_hash   BLOB    NOT NULL,          -- blake3(raw bytes), 32 bytes
  status         TEXT    NOT NULL DEFAULT 'ok' CHECK (status IN
                   ('ok','parse_error','duplicate_page','too_large','unsupported')),
  error          TEXT,
  parser_version INTEGER NOT NULL,
  indexed_at     INTEGER NOT NULL           -- unix ms
);

-- Last indexed content (zstd). Baseline for UUID carry-over diffs,
-- "modified on disk" diffs and 3-way merges. Optional (see Open questions).
-- NOTE: dropped by ADR-017 (merge base kept in memory). Kept here for reference only; do not implement.
CREATE TABLE file_snapshots (
  file_id  INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
  content  BLOB NOT NULL
);

-- ---------------------------------------------------------------------
-- Pages (one row per distinct page name: file-backed or placeholder)
-- ---------------------------------------------------------------------
CREATE TABLE pages (
  id                  INTEGER PRIMARY KEY,
  name                TEXT    NOT NULL UNIQUE,  -- page-name-sanity-lc
  original_name       TEXT    NOT NULL,         -- display name
  uuid                TEXT    NOT NULL UNIQUE,  -- UUIDv5(name)
  file_id             INTEGER REFERENCES files(id) ON DELETE SET NULL,  -- NULL = placeholder
  format              TEXT,
  is_journal          INTEGER NOT NULL DEFAULT 0,
  journal_day         INTEGER,                  -- yyyyMMdd
  namespace_parent_id INTEGER REFERENCES pages(id) ON DELETE SET NULL,
  is_builtin          INTEGER NOT NULL DEFAULT 0,  -- TODO, DONE, A, B, C, Contents, ...
  is_whiteboard       INTEGER NOT NULL DEFAULT 0,
  created_at          INTEGER,                  -- from properties, else file birth time
  updated_at          INTEGER,                  -- from properties, else file mtime
  search_title        TEXT    NOT NULL          -- normalize(original_name || ' ' || aliases)
);
CREATE INDEX pages_file          ON pages(file_id) WHERE file_id IS NOT NULL;
CREATE INDEX pages_journal_day   ON pages(journal_day) WHERE is_journal = 1;
CREATE INDEX pages_ns_parent     ON pages(namespace_parent_id) WHERE namespace_parent_id IS NOT NULL;
CREATE INDEX pages_original_nc   ON pages(original_name COLLATE NOCASE);

-- alias:: declared in the page file of page_id (directed as declared).
CREATE TABLE page_aliases (
  page_id        INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
  alias_page_id  INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
  source_file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  PRIMARY KEY (page_id, alias_page_id)
) WITHOUT ROWID;
CREATE INDEX page_aliases_rev ON page_aliases(alias_page_id, page_id);

-- tags:: declared in the page file of page_id.
CREATE TABLE page_tags (
  page_id        INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
  tag_page_id    INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
  source_file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  PRIMARY KEY (page_id, tag_page_id)
) WITHOUT ROWID;
CREATE INDEX page_tags_rev ON page_tags(tag_page_id, page_id);

-- ---------------------------------------------------------------------
-- Blocks (outline as pre-order intervals within a file)
-- ---------------------------------------------------------------------
CREATE TABLE blocks (
  id            INTEGER PRIMARY KEY,        -- internal, reassigned on reparse
  uuid          TEXT    NOT NULL UNIQUE,
  uuid_source   INTEGER NOT NULL,           -- 0 generated, 1 id:: property, 2 carried
  file_id       INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  page_id       INTEGER NOT NULL REFERENCES pages(id),
  parent_id     INTEGER REFERENCES blocks(id),  -- NULL = top level (parent is the page)
  ord           INTEGER NOT NULL,           -- pre-order index in file (pre-block = 0)
  subtree_end   INTEGER NOT NULL,           -- max ord within this block's subtree
  depth         INTEGER NOT NULL,           -- 1 = top level
  sibling_idx   INTEGER NOT NULL,           -- 0-based among siblings
  is_pre_block  INTEGER NOT NULL DEFAULT 0,
  format        TEXT    NOT NULL,           -- 'markdown' | 'org'
  content       TEXT    NOT NULL,           -- == Logseq :block/content (raw, de-indented)
  title         TEXT    NOT NULL,           -- first line w/o marker/priority, for display
  search_text   TEXT    NOT NULL,           -- normalized for FTS (§6.1)
  marker        TEXT,                       -- TODO DOING DONE LATER NOW WAIT WAITING
                                            -- CANCELED CANCELLED IN-PROGRESS
  priority      TEXT,                       -- A B C
  scheduled     INTEGER,                    -- yyyyMMdd
  scheduled_raw TEXT,                       -- full timestamp incl. time/repeater
  deadline      INTEGER,
  deadline_raw  TEXT,
  repeated      INTEGER NOT NULL DEFAULT 0,
  collapsed     INTEGER NOT NULL DEFAULT 0,
  heading       INTEGER,                    -- markdown heading size, or NULL
  created_at    INTEGER,                    -- from created-at:: (ms) when integer
  updated_at    INTEGER,
  byte_start    INTEGER NOT NULL,           -- span in file (UTF-8 bytes), for editor patches
  byte_end      INTEGER NOT NULL,
  line_start    INTEGER NOT NULL,
  content_hash  BLOB    NOT NULL,           -- blake3(content)[..16], for carry-over diff
  UNIQUE (file_id, ord)
);
CREATE INDEX blocks_page        ON blocks(page_id, file_id, ord);
CREATE INDEX blocks_parent      ON blocks(parent_id, sibling_idx);
CREATE INDEX blocks_pre         ON blocks(page_id) WHERE is_pre_block = 1;
CREATE INDEX blocks_marker      ON blocks(marker, page_id) WHERE marker IS NOT NULL;
CREATE INDEX blocks_priority    ON blocks(priority) WHERE priority IS NOT NULL;
CREATE INDEX blocks_scheduled   ON blocks(scheduled) WHERE scheduled IS NOT NULL;
CREATE INDEX blocks_deadline    ON blocks(deadline)  WHERE deadline  IS NOT NULL;
CREATE INDEX blocks_collapsed   ON blocks(file_id, ord, subtree_end) WHERE collapsed = 1;
CREATE INDEX blocks_created     ON blocks(created_at) WHERE created_at IS NOT NULL;
CREATE INDEX blocks_updated     ON blocks(updated_at) WHERE updated_at IS NOT NULL;

-- ---------------------------------------------------------------------
-- References
-- ---------------------------------------------------------------------
-- Logseq :block/refs (page part). kind lets the UI/explain distinguish sources;
-- queries use DISTINCT (block_id, page_id).
CREATE TABLE block_page_refs (
  block_id INTEGER NOT NULL REFERENCES blocks(id) ON DELETE CASCADE,
  page_id  INTEGER NOT NULL REFERENCES pages(id),
  kind     INTEGER NOT NULL,
  -- 1 [[link]]  2 #tag  3 property value  4 property name  5 marker
  -- 6 priority  7 namespace parent of a referenced page  8 {{embed [[p]]}}
  PRIMARY KEY (page_id, block_id, kind)
) WITHOUT ROWID;
CREATE INDEX block_page_refs_by_block ON block_page_refs(block_id, page_id);

-- Logseq :block/refs (block part): ((uuid)), {{embed ((uuid))}}, [x](((uuid)))
-- target is a UUID string, resolved by join on blocks.uuid (may dangle).
CREATE TABLE block_block_refs (
  block_id    INTEGER NOT NULL REFERENCES blocks(id) ON DELETE CASCADE,
  target_uuid TEXT    NOT NULL,
  kind        INTEGER NOT NULL,             -- 1 ref, 2 embed, 3 link
  PRIMARY KEY (target_uuid, block_id, kind)
) WITHOUT ROWID;
CREATE INDEX block_block_refs_by_block ON block_block_refs(block_id);

-- ---------------------------------------------------------------------
-- Properties (EAV). Page properties = properties of the page's pre-block.
-- ---------------------------------------------------------------------
CREATE TABLE block_properties (
  block_id   INTEGER NOT NULL REFERENCES blocks(id) ON DELETE CASCADE,
  key        TEXT    NOT NULL,              -- normalized: lower, '_'/' ' -> '-'
  pos        INTEGER NOT NULL,              -- properties-order
  raw_key    TEXT    NOT NULL,
  raw_value  TEXT    NOT NULL,              -- properties-text-values
  value_type INTEGER NOT NULL,              -- 0 string 1 integer 2 boolean 3 refs(set)
  builtin    INTEGER NOT NULL DEFAULT 0,    -- 0 user, 1 editable built-in, 2 hidden built-in
  PRIMARY KEY (block_id, key)
) WITHOUT ROWID;
CREATE INDEX block_properties_key ON block_properties(key);

-- One row per scalar value or per element of a ref set.
CREATE TABLE block_property_values (
  block_id    INTEGER NOT NULL,
  key         TEXT    NOT NULL,
  value_norm  TEXT    NOT NULL,             -- lower+NFC; page-name-sanity-lc for refs
  value_num   REAL,                         -- integers/booleans(0/1), else NULL
  ref_page_id INTEGER REFERENCES pages(id),
  PRIMARY KEY (block_id, key, value_norm),
  FOREIGN KEY (block_id, key) REFERENCES block_properties(block_id, key) ON DELETE CASCADE
) WITHOUT ROWID;
CREATE INDEX bpv_key_value ON block_property_values(key, value_norm, block_id);
CREATE INDEX bpv_key_num   ON block_property_values(key, value_num) WHERE value_num IS NOT NULL;
CREATE INDEX bpv_ref_page  ON block_property_values(ref_page_id) WHERE ref_page_id IS NOT NULL;

-- ---------------------------------------------------------------------
-- Diagnostics (parse errors, duplicate titles/ids, invalid property keys...)
-- ---------------------------------------------------------------------
CREATE TABLE diagnostics (
  id       INTEGER PRIMARY KEY,
  file_id  INTEGER REFERENCES files(id) ON DELETE CASCADE,
  kind     TEXT    NOT NULL,   -- parse_error | duplicate_page | duplicate_block_id
                               -- | invalid_property | case_conflict | too_large
  severity INTEGER NOT NULL,   -- 0 info 1 warning 2 error
  line     INTEGER,
  message  TEXT    NOT NULL,
  data     TEXT                -- JSON
);
CREATE INDEX diagnostics_file ON diagnostics(file_id);

-- ---------------------------------------------------------------------
-- Full-text search (external content: text stored once, in blocks/pages)
-- ---------------------------------------------------------------------
CREATE VIRTUAL TABLE blocks_fts USING fts5(
  search_text,
  content = 'blocks', content_rowid = 'id',
  tokenize = 'unicode61 remove_diacritics 2',
  prefix = '2 3'
);
CREATE VIRTUAL TABLE blocks_fts_tri USING fts5(
  search_text,
  content = 'blocks', content_rowid = 'id',
  tokenize = 'trigram'              -- substring + CJK; >= 3 chars
);
CREATE VIRTUAL TABLE pages_fts USING fts5(
  search_title,
  content = 'pages', content_rowid = 'id',
  tokenize = 'trigram'
);

CREATE TRIGGER blocks_ai AFTER INSERT ON blocks BEGIN
  INSERT INTO blocks_fts(rowid, search_text)     VALUES (new.id, new.search_text);
  INSERT INTO blocks_fts_tri(rowid, search_text) VALUES (new.id, new.search_text);
END;
CREATE TRIGGER blocks_ad AFTER DELETE ON blocks BEGIN
  INSERT INTO blocks_fts(blocks_fts, rowid, search_text)         VALUES ('delete', old.id, old.search_text);
  INSERT INTO blocks_fts_tri(blocks_fts_tri, rowid, search_text) VALUES ('delete', old.id, old.search_text);
END;
CREATE TRIGGER blocks_au AFTER UPDATE OF search_text ON blocks BEGIN
  INSERT INTO blocks_fts(blocks_fts, rowid, search_text)         VALUES ('delete', old.id, old.search_text);
  INSERT INTO blocks_fts_tri(blocks_fts_tri, rowid, search_text) VALUES ('delete', old.id, old.search_text);
  INSERT INTO blocks_fts(rowid, search_text)     VALUES (new.id, new.search_text);
  INSERT INTO blocks_fts_tri(rowid, search_text) VALUES (new.id, new.search_text);
END;
CREATE TRIGGER pages_ai AFTER INSERT ON pages BEGIN
  INSERT INTO pages_fts(rowid, search_title) VALUES (new.id, new.search_title);
END;
CREATE TRIGGER pages_ad AFTER DELETE ON pages BEGIN
  INSERT INTO pages_fts(pages_fts, rowid, search_title) VALUES ('delete', old.id, old.search_title);
END;
CREATE TRIGGER pages_au AFTER UPDATE OF search_title ON pages BEGIN
  INSERT INTO pages_fts(pages_fts, rowid, search_title) VALUES ('delete', old.id, old.search_title);
  INSERT INTO pages_fts(rowid, search_title) VALUES (new.id, new.search_title);
END;

-- ---------------------------------------------------------------------
-- Derived views
-- ---------------------------------------------------------------------
-- Logseq :block/path-refs = {page} ∪ own refs ∪ ancestors' refs (pages only).
-- Ancestors share the file, so "b is in a's subtree" == same file and ord in [a.ord, a.subtree_end].
CREATE VIEW block_path_refs AS
  SELECT d.id AS block_id, r.page_id AS page_id
  FROM block_page_refs r
  JOIN blocks a ON a.id = r.block_id
  JOIN blocks d ON d.file_id = a.file_id AND d.ord BETWEEN a.ord AND a.subtree_end
  UNION
  SELECT b.id, b.page_id FROM blocks b;

CREATE VIEW page_properties AS
  SELECT b.page_id, p.key, p.raw_key, p.raw_value, p.value_type, p.builtin, p.pos
  FROM blocks b JOIN block_properties p ON p.block_id = b.id
  WHERE b.is_pre_block = 1;

CREATE VIEW page_property_values AS
  SELECT b.page_id, v.key, v.value_norm, v.value_num, v.ref_page_id
  FROM blocks b JOIN block_property_values v ON v.block_id = b.id
  WHERE b.is_pre_block = 1;

CREATE VIEW tasks AS
  SELECT b.id, b.uuid, b.page_id, b.file_id, b.marker, b.priority,
         b.scheduled, b.scheduled_raw, b.deadline, b.deadline_raw, b.repeated, b.title
  FROM blocks b WHERE b.marker IS NOT NULL;
```

The DDL above was smoke-tested on SQLite 3.45.1. The test covered schema creation, path-refs through the interval view, trigram substring `MATCH`, `unicode61` prefix `MATCH`, `ON DELETE CASCADE` of refs and properties, and FTS cleanup through the delete trigger. Trigram matching needs at least 3 characters, so shorter queries use the `LIKE` fallback.

### 3.1 Design notes

- **Why no materialized `path_refs` table.** Logseq stores `:block/path-refs` on every block and has to recompute descendants whenever an outliner op changes refs or moves blocks (`src/main/frontend/modules/outliner/pipeline.cljs:27-83`). In Bitacora a file is always replaced as a whole, so the interval join gives the same set with no extra storage. It is driven by `block_page_refs(page_id)` → `blocks(id)` → range on `blocks(file_id, ord)` (the `UNIQUE (file_id, ord)` index). If benchmarks show the view is too slow for `(and [[a]] [[b]] [[c]])` over very large graphs, add `block_path_refs_mat(page_id, block_id)`, computed by the parser per file. It stays file-local, so per-file replace still works. **Decision (ADR-026, 2026-10-07): not needed.** On the 540k-block `large` benchmark the view answers a 3-reference AND query in 18 ms p95 (target 200 ms); see [[performance-report-1.0]]. Revisit only if a future benchmark misses the target.
- **Why tasks are columns, not a table.** Task attributes are 1:1 with blocks and are filtered together with other block predicates. Partial indexes keep them cheap, and the `tasks` view gives the requested surface.
- **Why `block_block_refs.target_uuid` is not a foreign key.** The target block may be in a file that hasn't been parsed yet, in a deleted file, or it may have been reparsed under a new rowid. Joining on `blocks.uuid` resolves it lazily, which is what Logseq's pre-created `{:block/uuid}` stubs approximate (`deps/graph-parser/src/logseq/graph_parser.cljs:109-115`).
- **`content` vs `search_text`.** `content` is exactly Logseq's `:block/content`, used for display, the editor, query predicates and export. `search_text` is derived for FTS (§6.1). External-content FTS keeps a single copy of the text. Only the inverted indexes are extra.
- **Placeholder pages.** These are pages created by refs, aliases, tags, namespace parents or property names, with `file_id IS NULL`. Logseq has the same thing: pages without `:block/file`. Built-in pages (`TODO`, `DONE`, `A`, `B`, `C`, `Contents`, `Favorites`, `card`; `deps/db/src/logseq/db/default.cljs:6-23`) are seeded with `is_builtin = 1` and never GC'ed.

---

## 4. Indexing pipeline

```
 ┌───────────────┐   paths    ┌──────────────┐  (path, bytes, hash) ┌────────────────┐  ParsedFile  ┌──────────────┐
 │ startup scan  │──────────▶│ change filter │────────────────────▶│ parse workers  │────────────▶│ writer thread│──▶ SQLite (WAL)
 │ notify watcher│ debounced  │ stat + blake3 │  only if hash differs│ (rayon, pure)  │  ordered by  │ 1 tx / file  │
 └───────────────┘            └──────────────┘                      └────────────────┘  priority    │ or batch     │
        ▲                              ▲                                                            └──────┬───────┘
        │ overflow/rescan              │ expected-hash registry (own writes)                               │ events
        └──────────────────────────────┴───────────────────────────────────────────────────────────────────▼
                                                                                                UI: page/blocks changed
```

### 4.1 Startup reconcile (cold or warm)

1. **Open and validate.** If `user_version ≠ SCHEMA_VERSION`, or `meta.parser_version ≠ PARSER_VERSION`, or `meta.config_hash ≠ hash(relevant config)`, truncate everything and do a full rebuild. On `SQLITE_CORRUPT` or a failed `quick_check`, delete the file and rebuild.
   - The **relevant config** is `:journal/page-title-format`, `:file/name-format`, `:journals-directory`, `:pages-directory`, `:whiteboards-directory`, `:hidden`, `:property/separated-by-commas`, `:property-pages/enabled?`, `:property-pages/excludelist`, `:ignored-page-references-keywords`, `:preferred-format`. Changing `normalizer_version` (accent folding) only recomputes `search_text` and runs `INSERT INTO blocks_fts(blocks_fts) VALUES('rebuild')`.
2. **Walk the graph.** Apply Logseq's ignore rules (`deps/common/src/logseq/common/graph.cljs:42-67`) and `:hidden`, then collect `{path, size, mtime_ns}`.
3. **Deleted files** (`db − disk`): run `delete_file(path)` (§4.4).
4. **For each disk file:**
   - If the row exists and `size` and `mtime_ns` are equal, skip it. This trusts mtime. A `paranoid_scan` setting hashes everything, and a low-priority background verifier rehashes files over time.
   - Otherwise read the bytes and compute blake3. If the hash equals `content_hash`, update `size` and `mtime_ns` only (touch, `git checkout` of identical content).
   - Otherwise enqueue a parse job.
5. **Priority:** today's journal and the configured home page first, then any page the UI requests, then `journals/` newest first, then `pages/`. This mirrors Logseq's homepage preload (`src/main/frontend/fs/watcher_handler.cljs:139-190`) and parse order (`deps/graph-parser/src/logseq/graph_parser.cljs:142-148`).
6. **Cold build fast path** (empty DB): drop the FTS triggers, run `foreign_keys=OFF`, insert in batches of about 200 files per transaction (Logseq batches 100, `src/main/frontend/handler/repo.cljs:240`), then `INSERT INTO blocks_fts(blocks_fts) VALUES('rebuild')` (same for `blocks_fts_tri` and `pages_fts`), recreate the triggers, `foreign_keys=ON`, `PRAGMA foreign_key_check`, and `ANALYZE`.

### 4.2 Watcher

- Use the `notify` crate (FSEvents / inotify / ReadDirectoryChangesW) with `notify-debouncer-full` at about **300 ms**. That plays the role of chokidar's `awaitWriteFinish` (`src/electron/electron/fs_watcher.cljs:73`).
- Create and modify events go to the change filter (stat + hash, §4.1 step 4).
- For a remove event, wait **500 ms** and re-stat. If the file exists again, treat the event as a modify (atomic save). If it is still gone, run `delete_file`. This is Logseq's unlink delay (`fs_watcher.cljs:91-97`).
- Rename `(from → to)`: if the hash of `to` equals the hash stored for `from`, update `files.path` and reparse `to` with carry-over from `from`, so UUIDs survive.
- Case-only rename on case-insensitive filesystems: detect a `path` that differs only by case and update it in place. Logseq does this in `validate-existing-file` (`src/main/frontend/handler/common/file.cljs:24-49`).
- On watcher overflow, `MustScanSubDirs`, a removed graph root or a remounted volume, mark the graph "stale" and run the startup reconcile.

### 4.3 Parse (pure, parallel)

`parse(path, bytes, config) -> ParsedFile`. It touches no DB and is deterministic apart from `uuid_source = 0` UUIDs, which the writer assigns.

```rust
struct ParsedFile {
    page: PageDef,                // name, original_name, journal_day, namespace chain,
                                  // aliases, tags, format, created/updated
    blocks: Vec<ParsedBlock>,     // pre-order; ord, subtree_end, depth, parent_ord,
                                  // content, title, search_text, marker, priority,
                                  // scheduled/deadline(+raw), repeated, collapsed, heading,
                                  // explicit_uuid, properties[], page_refs[], block_refs[],
                                  // byte span, line, content_hash
    referenced_pages: Vec<PageRefName>, // all names to upsert (incl. namespace parents,
                                        // property names, markers, priorities)
    diagnostics: Vec<Diagnostic>,
}
```

The parser reproduces Logseq's rules (see [[03-parsing-indexing-search]] §3–§5 and [[02-markdown-block-syntax]]): page name precedence, the journal formats, `with-parent-and-left`, the pre-block, property parsing, refs including markers, priorities and namespace parents, and `id::`.

### 4.4 Writer: transactional per-file replace

```sql
BEGIN IMMEDIATE;
-- 0. remember GC candidates
--    old_page := pages.id WHERE file_id = :file_id
--    old_ref_pages := DISTINCT page_id FROM block_page_refs JOIN blocks ... WHERE file_id = :file_id
--                    ∪ alias/tag pages with source_file_id = :file_id
--    old_blocks := (ord, depth, content_hash, uuid, uuid_source) for carry-over
-- 1. file row
INSERT INTO files(path, kind, format, size, mtime_ns, content_hash, status, parser_version, indexed_at)
VALUES (...) ON CONFLICT(path) DO UPDATE SET kind=excluded.kind, ..., status='ok', error=NULL
RETURNING id;
-- 2. drop everything derived from this file (FTS via triggers; refs/props via CASCADE)
DELETE FROM blocks       WHERE file_id = :file_id;
DELETE FROM page_aliases WHERE source_file_id = :file_id;
DELETE FROM page_tags    WHERE source_file_id = :file_id;
DELETE FROM diagnostics  WHERE file_id = :file_id;
-- 3. if the file defined a page that it no longer defines (title:: changed) -> demote it
UPDATE pages SET file_id = NULL, format = NULL, created_at = NULL, updated_at = NULL
 WHERE file_id = :file_id AND name <> :new_page_name;
-- 4. upsert the defined page (+ namespace parents, as placeholders if new)
INSERT INTO pages(name, original_name, uuid, file_id, format, is_journal, journal_day,
                  namespace_parent_id, created_at, updated_at, search_title)
VALUES (...)
ON CONFLICT(name) DO UPDATE SET
  original_name = excluded.original_name,
  file_id = COALESCE(pages.file_id, excluded.file_id),   -- first file wins (duplicate title)
  ...;
--    if pages.file_id <> :file_id afterwards -> files.status='duplicate_page' + diagnostic
-- 5. upsert referenced pages (INSERT ... ON CONFLICT(name) DO NOTHING) -> name→id map
-- 6. insert blocks in pre-order (parent_id resolved from parent_ord via map), with uuids
--    assigned by: explicit id:: > carry-over diff > new UUIDv7; check global uniqueness
-- 7. insert block_page_refs, block_block_refs, block_properties, block_property_values
-- 8. insert page_aliases / page_tags with source_file_id = :file_id
-- 9. recompute pages.search_title for the defined page and its alias pages
-- 10. GC: delete candidate pages that are now unreferenced
DELETE FROM pages
 WHERE id IN (/* candidates */)
   AND file_id IS NULL AND is_builtin = 0
   AND NOT EXISTS (SELECT 1 FROM block_page_refs      r WHERE r.page_id = pages.id)
   AND NOT EXISTS (SELECT 1 FROM block_property_values v WHERE v.ref_page_id = pages.id)
   AND NOT EXISTS (SELECT 1 FROM page_aliases a WHERE pages.id IN (a.page_id, a.alias_page_id))
   AND NOT EXISTS (SELECT 1 FROM page_tags    t WHERE pages.id IN (t.page_id, t.tag_page_id))
   AND NOT EXISTS (SELECT 1 FROM pages c WHERE c.namespace_parent_id = pages.id)
   AND NOT EXISTS (SELECT 1 FROM blocks b WHERE b.page_id = pages.id);
-- 11. snapshot (optional)
INSERT OR REPLACE INTO file_snapshots(file_id, content) VALUES (:file_id, zstd(:bytes));
COMMIT;
```

`delete_file(path)` runs the same steps 0, 2, 3 (demote the page unconditionally), deletes the `files` row (cascading to the snapshot) and runs GC. This matches Logseq's `retract-page-attributes` (`deps/db/src/logseq/db/schema.cljs:131-142`): a page whose file is gone stays as a placeholder if it is still referenced.

After commit, the writer emits `IndexEvent::FileReplaced { file_id, page_ids_touched, block_uuids_added/removed }` so the UI can refresh open pages, live queries and backlinks panels. This is the equivalent of Logseq's affected-keys refresh (`src/main/frontend/db/react.cljs:237-361`).

### 4.5 Own writes and "modified on disk" conflicts

1. Before writing file F, the editor layer stats and hashes the disk copy. If the hash differs from `files.content_hash`, the file was **modified on disk**. Bitacora must not write. It shows a diff against `file_snapshots`, or does a 3-way merge with the editor buffer. Logseq's equivalent is `:file/not-matched-from-disk` (`src/main/frontend/fs/node.cljs:44-50`, `src/main/frontend/handler/events.cljs:374-380`).
2. Write atomically (temp file + rename). Register `expected[path] = new_hash`, then immediately run the pipeline on the in-memory bytes, so the UI never waits for the watcher.
3. When the watcher event arrives with `hash == files.content_hash`, it is a no-op apart from updating size and mtime.
4. Whenever Bitacora replaces disk content that it did not index (forced overwrite), back up the previous version to `logseq/bak/<path-without-ext>/<ISO-ts with : → _>.Desktop.<ext>` and keep the 6 newest, for Logseq compatibility (`src/electron/electron/backup_file.cljs:7-51`).

### 4.6 Size and throughput expectations

These are rough estimates to validate with benchmarks, not measurements.

| Graph | Blocks | Text | Est. DB size (both block FTS) | Cold build target |
|---|---|---|---|---|
| Medium | 50 k | 6 MB | ~40 MB | < 5 s |
| Large | 500 k | 60 MB | ~400 MB (trigram is about 3–4× text) | < 60 s |

A per-file replace of a typical page (< 500 blocks) should take under 10 ms. Trigram can be disabled with a setting (`search.substring = false`) to roughly halve the DB size.

### 4.7 Implementation notes (BIT-US-0006, BIT-US-0007)

Implemented in `crates/bitacora-index` (`replace.rs`, `writer.rs`, `reconcile.rs`, `carry.rs`). Where it refines the text above:

- **Retain in place.** Step 2 does not delete blocks that the carry-over diff found *identical* (same depth and content hash, not pre-blocks): their rows keep `id`, FTS entries, refs and properties and only get new `ord`/`subtree_end`/`parent_id`/`sibling_idx`/byte spans. Everything else is deleted and inserted. A 500-block page with one edited block replaces in about 3 ms (release build) instead of about 29 ms with delete-all-and-insert, mostly because the trigram FTS is untouched.
- **Duplicate page titles (open question 4).** The file with the smallest path owns the page (`pages.file_id`, title, journal day, created/updated); other files are marked `duplicate_page` with a diagnostic and their blocks stay listed under the page. Ownership is order independent, so a rebuild equals an incremental run. After a delete/rename/edit that frees or lowers ownership, `Indexer::refresh_duplicates` re-evaluates the affected `duplicate_page` files. Explicit `id::` clashes across files are still first-writer-wins (§2.2) and therefore order dependent.
- **Page times.** `created_at` falls back to the file birth time and `updated_at` to the file mtime when no `created-at::`/`updated-at::` property exists; a touch (identical content, new mtime) moves a mtime-derived `updated_at`.
- **Cold build.** `begin_bulk` sets `meta.bulk_in_progress`, drops the FTS triggers and turns foreign keys off; `end_bulk` commits, runs `'rebuild'` on the three FTS tables, recreates the triggers, runs `foreign_key_check` and `ANALYZE`, then clears the flag. A crash in between leaves the flag, and the next reconcile truncates and rebuilds. `RebuildKind::FtsOnly` recomputes `search_text`/`search_title` and rebuilds the FTS tables without reparsing.
- **Events.** `IndexEvent::{FileReplaced, FileDeleted, FileRenamed, BulkFinished}` are sent after commit; none during a bulk run except `BulkFinished`. Watcher input is the `FsChange` enum (`Modified`, `Deleted`, `Renamed`, `Overflow`); watcher events always hash the file instead of trusting `(size, mtime_ns)`.
- **Measured** (24-core Linux, release): cold build of a synthetic 50 080-block / 1 200-file graph in 0.83 s (37 MiB index), warm reconcile of the unchanged graph in 2.7 ms with 0 files parsed.

---

## 5. Core read queries

```sql
-- Page outline (lazy: first N visible blocks, skipping collapsed subtrees)
SELECT b.* FROM blocks b
WHERE b.page_id = :page_id
  AND NOT EXISTS (SELECT 1 FROM blocks c
                  WHERE c.file_id = b.file_id AND c.collapsed = 1
                    AND b.ord > c.ord AND b.ord <= c.subtree_end)
ORDER BY b.file_id, b.ord
LIMIT :n OFFSET :k;

-- Subtree of a block
SELECT d.* FROM blocks a JOIN blocks d
  ON d.file_id = a.file_id AND d.ord BETWEEN a.ord AND a.subtree_end
WHERE a.uuid = :uuid ORDER BY d.ord;

-- Ancestors (breadcrumb)
SELECT x.* FROM blocks b JOIN blocks x
  ON x.file_id = b.file_id AND x.ord < b.ord AND x.subtree_end >= b.ord
WHERE b.uuid = :uuid ORDER BY x.ord;

-- Alias closure (symmetric, 2 hops, as Logseq's `alias` rule: deps/db/src/logseq/db/rules.cljc:14-24)
WITH RECURSIVE al(id, hops) AS (
  SELECT :page_id, 0
  UNION
  SELECT CASE WHEN a.page_id = al.id THEN a.alias_page_id ELSE a.page_id END, al.hops + 1
  FROM page_aliases a JOIN al ON al.id IN (a.page_id, a.alias_page_id)
  WHERE al.hops < 2)
SELECT DISTINCT id FROM al;

-- Linked references (Logseq model.cljs:1255-1283): path-refs ∩ alias set, minus the page's own blocks
WITH targets(id) AS (/* alias closure */)
SELECT DISTINCT d.id, d.page_id, d.file_id, d.ord
FROM block_page_refs r
JOIN blocks a ON a.id = r.block_id
JOIN blocks d ON d.file_id = a.file_id AND d.ord BETWEEN a.ord AND a.subtree_end
WHERE r.page_id IN (SELECT id FROM targets)
  AND d.page_id <> :page_id                 -- Logseq excludes only the page itself
ORDER BY d.page_id, d.file_id, d.ord;
-- The UI then keeps only top-most matches per subtree and groups them by page.
-- Logseq's `filters::` page property (include/exclude ref pages) adds:
--   AND d.id IN / NOT IN (SELECT block_id FROM block_path_refs WHERE page_id = :filter_page)

-- Block references count / list
SELECT b.* FROM block_block_refs r JOIN blocks b ON b.id = r.block_id WHERE r.target_uuid = :uuid;

-- Unlinked references (Logseq model.cljs:1320-1350, without the full scan):
SELECT b.id, b.content FROM blocks_fts f JOIN blocks b ON b.id = f.rowid
WHERE blocks_fts MATCH :quoted_name_or_aliases   -- '"page name" OR "alias"'
  AND b.page_id <> :page_id;
-- then, in Rust, apply Logseq's regex
--   (?i)(^|[^\[#0-9a-zA-Z]|((^|[^\[])\[))NAME($|[^0-9a-zA-Z])
-- on content with the logbook drawer removed.

-- Global graph edges (Logseq model.cljs:1178-1199; refs, not path-refs)
SELECT DISTINCT b.page_id AS src, r.page_id AS dst
FROM block_page_refs r JOIN blocks b ON b.id = r.block_id
WHERE b.page_id <> r.page_id;
-- + namespace edges: SELECT id, namespace_parent_id FROM pages WHERE namespace_parent_id IS NOT NULL
-- + filters: journals, orphans, is_builtin, page property exclude-from-graph-view = true

-- Namespace tree (recursive, Logseq rules.cljc:7-12)
WITH RECURSIVE ns(id, depth) AS (
  SELECT id, 0 FROM pages WHERE name = :ns
  UNION ALL
  SELECT p.id, ns.depth + 1 FROM pages p JOIN ns ON p.namespace_parent_id = ns.id)
SELECT p.* FROM ns JOIN pages p ON p.id = ns.id;

-- Journal agenda: scheduled/deadline ≤ today+N, not done (Logseq model.cljs:1285-1318)
SELECT * FROM tasks
WHERE (scheduled BETWEEN :today AND :future OR deadline BETWEEN :today AND :future
       OR (repeated = 1 AND COALESCE(scheduled, deadline) <= :future))
  AND marker NOT IN ('DONE','CANCELED','CANCELLED');
```

### 5.1 Implementation notes (BIT-US-0008, BIT-US-0010)

`bitacora-index::read::IndexReader` (`index.read_api()`) exposes the queries above as typed functions; no `rusqlite` type leaks.

- **Linked references** return the blocks whose *own* refs hit the alias closure (outside the page), keep only the top-most block of nested hits, and attach a breadcrumb; descendants are reached with `subtree`. `filters::` is a map of page title to boolean (`true` include: every included page must be in the block's path-refs; `false` exclude), as Logseq's `filter-blocks` does. Filters are evaluated with an interval `EXISTS` over `block_page_refs`, not through the `block_path_refs` view.
- **Kind-agnostic refs.** `block_page_refs.kind` never takes part in reference queries: a page written as `[[x]]`, `#x` and `#[[x]]` in one block is one `:block/refs` entry, and a block is found by any spelling (tested). The ingest records one content-ref kind per (block, page) pair (`Tag` when `#x` appears, else `Link`).
- **Unlinked references** run the FTS prefilter as `blocks_fts f CROSS JOIN blocks b ON b.id = f.rowid` without `ORDER BY` (ordering is done in Rust) so the planner cannot walk `blocks`; the Logseq regex runs on content with `:LOGBOOK:` drawers removed; blocks that already reference the page or one of its aliases are dropped. Names whose normalised form has no word characters (and CJK-only names, which `unicode61` tokenises as one token) are not found by the prefilter.
- **Agenda** follows the query in this section (tasks only, window `[today, today + N]`, repeating tasks whose first date is not after the window end).
- **Doctor.** `inspect_index(db_path)` never goes through `Index::open` (which deletes corrupt files); it runs `quick_check`, `foreign_key_check` and the FTS5 `integrity-check` of `blocks_fts`, `blocks_fts_tri`, `pages_fts` on a read-write connection and reports instead of repairing. `DiagnosticKind` gained `DuplicatePage` and `CaseConflict`.
- **Graph view data (BIT-US-0155).** `IndexReader::graph_data(&GraphFilter)` and `local_graph_data(page_id, &GraphFilter)` (`read/graph.rs`) return `GraphData { nodes{id,name,is_journal,is_tag,is_namespace_parent,degree}, edges }`. Edges are the distinct, directed, non-self `block_page_refs` of kind link/tag/property-value/embed (source = the block's page), `page_tags` and namespace child -> parent; placeholder pages are nodes. Excluded: UUID-like names, `assets/...` names, pages with `exclude-from-graph-view:: true`, the configured excluded pages, journals / built-ins per filter, and (global graph, `orphans = false`) nodes without edges. `degree` counts incident edges among the visible nodes. The local graph is the centre (always kept) plus its 1-hop neighbours in either direction and the links among them. The older `graph_edges` stays for existing callers. Consumed by `bitacora-graph` through the app/runtime (ADR-034).
- **CLI.** `bitacora-cli reindex|doctor --graph <path> [--data-dir <dir>] [--json]`; `doctor` exits 2 when the index is missing or a check fails.

---

## 6. Search

### 6.1 Normalization (`search_text`, `search_title`, queries)

`normalize(s)` = remove built-in property lines (`id::`, `collapsed::`, `heading::`, `created-at::`, …, as in Logseq `property/remove-built-in-properties`, `src/main/frontend/search/db.cljs:23-35`) → NFKC → lower-case → (optional, default on) strip diacritics, matching `:feature/enable-search-remove-accents?` in `src/main/frontend/util.cljc:1024-1031`. Blocks longer than `search.max_block_len` (default 10 000 chars, Logseq `src/main/frontend/state.cljs:680-682`) are truncated for indexing and flagged with a diagnostic, not dropped. Queries go through the same `normalize`.

### 6.2 Query pipeline

This follows Logseq `master:src/main/frontend/worker/search.cljs:991-1068`:

1. **Parse the user input.** ` and `/`&`, ` or `/`|` and ` not ` become FTS5 operators (`src/electron/electron/search.cljs:244-255`). Every other token is double-quoted to neutralize FTS syntax. Input containing punctuation is turned into a phrase.
2. **Exact page title or alias** (`pages.name = :q` or alias name) ranks first.
3. **Page title search:** `pages_fts MATCH :q` (trigram, needs ≥ 3 chars), plus an in-memory **subsequence fuzzy** pass over all titles in Rust (`nucleo-matcher`, or a port of Logseq's scorer in `src/main/frontend/search.cljs:51-90`).
4. **Block word search:**
   ```sql
   SELECT rowid, bm25(blocks_fts) AS score FROM blocks_fts
   WHERE blocks_fts MATCH :q ORDER BY score LIMIT :k;
   ```
5. **Block substring search** (q ≥ 3 chars):
   ```sql
   SELECT rowid FROM blocks_fts_tri WHERE blocks_fts_tri MATCH :quoted_q LIMIT :k;
   ```
   For q < 3 chars: `SELECT id FROM blocks WHERE search_text LIKE :pat ESCAPE '\' LIMIT :k`.
6. **Merge** with Reciprocal Rank Fusion, `k = 60` (`master:src/main/frontend/worker/search.cljs:738-766`). Boost pages over blocks and slightly boost recent journal days. Deduplicate by block or page id.
7. **Snippets** are built in Rust from `content`: windows around the matched terms, with highlight markers. FTS5 `snippet()` is not usable with two tokenizers, and Logseq `master` dropped it too (`master:src/main/frontend/worker/search.cljs:204-400`).
8. **Scopes:** `page = :page_id` (search within page), and journals only or pages only (`pages.is_journal`).

Property names, property values and templates are not searched through FTS. They come from small `SELECT DISTINCT key FROM block_properties` and `SELECT DISTINCT value_norm FROM block_property_values WHERE key = ?` queries plus the fuzzy matcher, as Logseq does (`src/main/frontend/search.cljs:184-215`).

### 6.3 Implementation notes (BIT-US-0009)

Code: `crates/bitacora-index/src/search/` (`query`, `fuzzy`, `snippet`, `mod`); entry point `search::search(&Connection, &str, &SearchOptions)`.

- **Parser.** Whitespace tokens, `"phrases"`, `and`/`&`, `or`/`|`, `not`; every term is folded with the index normalizer and double-quoted (`"` doubled); tokens without a letter or digit are dropped; leading/trailing/doubled operators are dropped (`a and not b` is `a NOT b`). The last plain word gets a prefix `*` for the word index.
- **Stages.** Exact title or alias (`pages.name`, alias directed as declared), `pages_fts` trigram (or `LIKE` on `search_title` under three characters), nucleo fuzzy over `pages.search_title` (two or more characters, atoms ANDed), bm25 on `blocks_fts`, and, only when the word search returned fewer than `limit` blocks, trigram on `blocks_fts_tri` (all positive terms of three or more characters) or `LIKE` (short terms, or substring disabled).
- **Fusion.** RRF `k = 60`; the exact-title list has weight 3, pages get a 1.25 factor over blocks, and with `SearchOptions::today` journal days from the last year get up to +15%.
- **Snippets.** Built from raw `content`: the folded text is matched and ranges are mapped back through a per-character offset map; window of 160 chars, `…` markers, newlines shown as spaces.
- **`search.substring`.** `IndexWriter::set_substring(bool)` (or `search::set_substring` on the write connection) drops `blocks_fts_tri` and switches the FTS triggers to the no-trigram variant, or recreates and rebuilds it; the cold-build fast path and `rebuild_fts` consult `sqlite_master`, so the choice survives bulk builds. The setting is not persisted: the caller applies it after `Index::open` (a recreated database starts with the trigram index).
- **Benchmark.** `tests/bench_search.rs` (ignored): 5,200 pages / 51,500 blocks, 14 queries x 20 runs, release build.

---

## 7. Simple query DSL → SQL

### 7.1 Compilation model

1. **Parse** the `{{query …}}` body with Logseq's pre-transform: `[[x]]` becomes a page ref, `#x` becomes a tag ref, and `between` arguments become keywords (`src/main/frontend/db/query_dsl.cljs:452-476`). Then read it as an EDN s-expression into an AST:
   `And(Vec) | Or(Vec) | Not(Vec) | PageRef(name) | Text(s) | Task(set) | Priority(set) | Property(k, Option<v>) | PageProperty(k, Option<v>) | Between(a, b) | BetweenProp(k, a, b) | Page(name) | Namespace(name) | PageTags(set) | AllPageTags | SortBy(k, dir) | Sample(n)`.
2. **Decide the result type** with Logseq's rule (`query_dsl.cljs:393-397`). The result is **blocks** if any of `PageRef` (outside `page-property`/`page-tags`), `Text`, `Between`, `Property`, `Task`, `Priority` or `Page` appears. Otherwise it is **pages**.
3. **Resolve names → page ids** before generating SQL (`SELECT id FROM pages WHERE name = ?`). An unknown name gives an empty set (`id IN (SELECT NULL WHERE 0)`). This keeps the SQL static and lets the planner use the indexes.
4. **Emit** each leaf as a predicate over the current row alias (`b` for blocks, `p` for pages). Combine with `AND`/`OR`/`NOT`. In block mode, page-level leaves are wrapped as `b.page_id IN (SELECT p.id FROM pages p WHERE <leaf>)`, which is Logseq's `[?b :block/page ?p]` binding (`query_dsl.cljs:478-503`).
5. **Post-process:** `SortBy` → `ORDER BY`. `Sample` → `ORDER BY random() LIMIT n`. Exclude the block that contains the query (`custom-query-result-transform` remove-blocks, `src/main/frontend/db/query_react.cljs:32-59`). Hydrate rows, group them by page or render them as a table (`query-table::`, `query-properties::`, `query-sort-by::`, `query-sort-desc::`).

### 7.2 Leaf mapping

| DSL | Logseq rule (`deps/db/src/logseq/db/rules.cljc`) | SQL predicate (block mode `b`, page mode `p`) |
|---|---|---|
| `[[x]]`, `#x` | `page-ref` via `:block/path-refs` (`:140-143`) | `b.id IN (SELECT d.id FROM block_page_refs r JOIN blocks a ON a.id=r.block_id JOIN blocks d ON d.file_id=a.file_id AND d.ord BETWEEN a.ord AND a.subtree_end WHERE r.page_id=:x UNION SELECT id FROM blocks WHERE page_id=:x)` |
| `"text"` | `block-content` = `clojure.string/includes?` (`:114-117`) | len ≥ 3: `b.id IN (SELECT rowid FROM blocks_fts_tri WHERE blocks_fts_tri MATCH :quoted)`. Else: `instr(b.search_text, :norm) > 0` |
| `(task TODO DOING)` | `task` (`:80-83`) | `b.marker IN ('TODO','DOING')` |
| `(priority a b)` | `priority` (`:85-88`) | `b.priority IN ('A','B')` |
| `(property k)` | `has-property` (`:108-112`) | `b.id IN (SELECT block_id FROM block_properties WHERE key=:k)` |
| `(property k v)` | `property`: `=` or `contains?` (`:129-138`) | `b.id IN (SELECT block_id FROM block_property_values WHERE key=:k AND value_norm=:v_norm)` (v parsed as in `query_dsl.cljs:242-263`: int/bool, `#x`→x, `[[x]]`→x) |
| `(page-property k)` | `has-page-property` (`:74-78`) | `p.id IN (SELECT page_id FROM page_properties WHERE key=:k)` |
| `(page-property k v)` | `page-property` (`:67-72`) | `p.id IN (SELECT page_id FROM page_property_values WHERE key=:k AND value_norm=:v_norm)` |
| `(between -7d today)` | `between` on journal page day (`:100-106`) | `b.page_id IN (SELECT id FROM pages WHERE is_journal=1 AND journal_day BETWEEN :a AND :b)` |
| `(between created-at -1d now)` | `get` on properties, ms (`query_dsl.cljs:214-229`) | `b.id IN (SELECT block_id FROM block_property_values WHERE key='created-at' AND value_num >= :a AND value_num < :b)`. **SHOULD** fall back to `b.created_at`/`b.updated_at` columns |
| `(page x)` | `page` (`:119-122`) | `b.page_id = :x` |
| `(namespace x)` | `namespace`, direct parent (`:124-127`) | `p.namespace_parent_id = :x` |
| `(page-tags a b)` | `page-tags` (`:90-94`) | `p.id IN (SELECT page_id FROM page_tags WHERE tag_page_id IN (:a,:b))` |
| `(all-page-tags)` | `all-page-tags` (`:96-98`) | `p.id IN (SELECT tag_page_id FROM page_tags)` |
| `(sort-by k [asc\|desc])` | post-sort by block property, default desc (`query_dsl.cljs:335-349`) | `ORDER BY (SELECT value_num FROM block_property_values v WHERE v.block_id=b.id AND v.key=:k) DESC NULLS LAST, (SELECT raw_value FROM block_properties WHERE block_id=b.id AND key=:k) DESC` |
| `(sample n)` | post-shuffle (`query_dsl.cljs:327-333`) | `ORDER BY random() LIMIT :n` |
| `(and …)` / `(or …)` / `(not …)` | Datalog `and`/`or`/`not` | `( … AND … )` / `( … OR … )` / `NOT ( … )` |

Date arguments (`today`, `yesterday`, `tomorrow`, `±N(d|w|m|y)`, `[[Journal title]]`, and `now` / `±Nh` / `±Nmin` for timestamps) resolve to `yyyyMMdd` ints or ms in Rust (`query_dsl.cljs:52-113`).

### 7.3 Examples

```sql
-- {{query (and [[project-x]] (task TODO DOING))}}
SELECT b.* FROM blocks b
WHERE b.id IN (SELECT d.id FROM block_page_refs r
                JOIN blocks a ON a.id = r.block_id
                JOIN blocks d ON d.file_id = a.file_id AND d.ord BETWEEN a.ord AND a.subtree_end
               WHERE r.page_id = 42
               UNION SELECT id FROM blocks WHERE page_id = 42)
  AND b.marker IN ('TODO','DOING')
ORDER BY b.page_id, b.file_id, b.ord;

-- {{query (and (page-property type book) (page-tags fiction))}}   -> pages
SELECT p.* FROM pages p
WHERE p.id IN (SELECT page_id FROM page_property_values WHERE key='type' AND value_norm='book')
  AND p.id IN (SELECT page_id FROM page_tags WHERE tag_page_id = 77);

-- {{query (and (between -7d today) (not (task DONE)) "standup")}}
SELECT b.* FROM blocks b
WHERE b.page_id IN (SELECT id FROM pages WHERE is_journal = 1 AND journal_day BETWEEN :today_minus_7 AND :today)
  AND NOT (b.marker IS NOT NULL AND b.marker IN ('DONE'))
  AND b.id IN (SELECT rowid FROM blocks_fts_tri WHERE blocks_fts_tri MATCH '"standup"');

-- {{query (and [[reading]] (property status active) (sort-by rating desc))}}
SELECT b.* FROM blocks b
WHERE b.id IN (/* path-refs of 'reading' */)
  AND b.id IN (SELECT block_id FROM block_property_values WHERE key='status' AND value_norm='active')
ORDER BY (SELECT value_num FROM block_property_values v WHERE v.block_id=b.id AND v.key='rating') DESC;
```

`NOT` must be NULL-safe. `b.marker IN (...)` is NULL for non-tasks, so `NOT (marker IN ('DONE'))` would drop them. Wrap every leaf as `COALESCE(<leaf>, 0)`, or emit `b.id IN (…)` forms that never yield NULL. Logseq's `not` keeps blocks without a marker.

### 7.4 Deliberate deviations (documented)

1. **`"text"` is case-insensitive and accent-insensitive** (FTS over normalized text). Logseq uses a case-sensitive `includes?` on raw content. Offer `(text-cs "…")` if strict behaviour is needed.
2. **Property value matching is case-insensitive** (`value_norm`). In Logseq, ref sets keep their original case, so `(property tags Book)` vs `tags:: book` is case-sensitive in practice.
3. **`[[x]]` keeps Logseq semantics**: it matches every block on page x and the descendants of referencing blocks. It does **not** expand aliases, as Logseq's `page-ref` rule doesn't (`rules.cljc:140-143`).

### 7.5 Implementation notes (BIT-US-0101)

Code: `crates/bitacora-index/src/query/` (`edn`, `dsl`, `dates`, `compile`, `mod`); entry points `IndexReader::query_simple(src, &QueryContext)` and the connection-free `query::compile_simple`.

- **Parser.** `pre_transform` rewrites `[[x]]`, `#x` and `#[[x]]` (outside strings) into the string `"[[x]]"`, then a small EDN reader builds the AST. The `{{query ...}}` wrapper is optional. Several top-level forms are an implicit `and` (Logseq reads only the first form). `sort-by` and `sample` are lifted out of the tree. Unknown operators fail with `unsupported: query operator ...`; a malformed query with `syntax error: ...`.
- **Names are resolved inside the SQL** (`(SELECT id FROM pages WHERE name = ?)`), so compilation needs no connection and an unknown page yields an empty set. Every value is a bound parameter.
- **NULL safety.** Leaves over nullable columns (`marker`, `priority`, `namespace_parent_id`, `page_id` equality) are wrapped in `COALESCE(..., 0)`; the other leaves are `IN (SELECT ...)` forms that never yield NULL. `(not a b)` negates the conjunction of its arguments (Datalog `not`).
- **Dates.** `QueryContext` carries today's date, `now_ms` and the start of today; `-Nh`/`-Nmin` count from the start of today like the day offsets (the documented DSL behaviour), `now` is `now_ms`. Journal titles use `journal_title_formatters`.
- **`between created-at|last-modified-at`** matches `block_property_values.value_num` in `[a, b)` or the `created_at` / `updated_at` columns. Other keys are `unsupported`.
- **`"text"`** uses `blocks_fts_tri` when it exists and the text has at least 3 characters, else `instr(search_text, ?)`; the reader turns the trigram path off when `search.substring` dropped the table.
- **`sort-by`** orders numbers first (`MIN(value_num)`), then text (`MIN(raw_value)`), NULLs last, default descending; `created-at` / `last-modified-at` fall back to the block columns. `sample n` is `ORDER BY random() LIMIT n` and replaces any sort. The block holding the query is excluded through `QueryContext::query_block`.

---

## 8. Advanced (Datalog) queries: supported subset

Compile `[:find … :in $ … :where …]` into SQL by treating each attribute as a binary relation `(e, v)`. Variables become join columns, constants become `WHERE` equalities and predicates become SQL expressions. This is a standard conjunctive-query compilation. Entities are typed (page or block), so each variable gets a type from the attributes that use it. Variables that could be either type are rejected with a clear error.

| Datalog attribute | Relation (e, v) |
|---|---|
| `:block/name`, `:block/original-name` | `pages(id, name)`, `pages(id, original_name)` |
| `:block/journal?`, `:block/journal-day` | `pages(id, is_journal)`, `pages(id, journal_day)` |
| `:block/namespace` | `pages(id, namespace_parent_id)` |
| `:block/alias`, `:block/tags` (page) | `page_aliases ∪ reverse`, `page_tags(page_id, tag_page_id)` |
| `:block/file` / `:file/path` | `pages(id, file_id)` / `files(id, path)` |
| `:block/uuid`, `:block/content`, `:block/format` | `blocks(id, uuid \| content \| format)` |
| `:block/page`, `:block/parent` | `blocks(id, page_id)`, `blocks(id, COALESCE(parent_id, page_id))` (typed union) |
| `:block/marker`, `:block/priority`, `:block/scheduled`, `:block/deadline`, `:block/repeated?`, `:block/collapsed?`, `:block/pre-block?` | block columns |
| `:block/refs` | `block_page_refs(block_id, page_id) ∪ block_block_refs⋈blocks(uuid)` |
| `:block/path-refs` | `block_path_refs(block_id, page_id)` |
| `:block/properties` + `[(get ?p :k) ?v]` | `block_property_values(block_id, key=:k, value)` (the `get` is fused with the attribute) |
| `:block/created-at`, `:block/updated-at` | block or page columns |

- **Predicates:** `=`, `not=`, `<`, `<=`, `>`, `>=`, `contains?` (set literal or input → `IN`), `get-else` (`LEFT JOIN` + `COALESCE`), `missing?` (`NOT EXISTS`), `clojure.string/includes?` / `starts-with?` / `ends-with?` (`instr`, `LIKE`), `re-find` + `re-pattern` (a `regexp()` UDF registered from Rust's `regex` crate), `str`, `untuple`.
- **Logic:** `not`, `not-join`, `or`, `or-join` (→ `UNION` / `NOT EXISTS`). The DSL rules (`rules.cljc:63-143`) are inlined. The recursive `namespace` and `alias` rules compile to `WITH RECURSIVE`.
- **Inputs:** every `:inputs` keyword from `deps/graph-parser/src/logseq/graph_parser/util/db.cljs:76-168` (`:current-page`, `:query-page`, `:current-block`, `:parent-block`, `:today`, `:±Nd`, `:±Nd-start`, `:today-HHMM`, `:right-now-ms`, …), plus `"[[page]]"` strings.
- **Find specs:** `?x`, `[?x ...]`, `(pull ?b [*])` (hydrate the block in Rust), `(pull ?p [*])`, scalar aggregates `(count ?b)`, `(min …)`, `(max …)` → SQL aggregates.
- **Not supported in v1:** `:result-transform` and `:view` (SCI code), arbitrary Clojure fns, `pull` patterns with nested reverse refs, rules not in the whitelist. These are reported as "unsupported", and the raw results are still shown when that is possible.

### 8.1 Implementation notes (BIT-US-0103)

Code: `crates/bitacora-index/src/query/` (`advanced`, `datalog`); entry point `IndexReader::query_advanced(src, &QueryContext) -> AdvancedOutcome`. Corpus: `fixtures/queries/advanced/*.edn` (43 queries written for this project, each with `;; class:` and expected rows), driven by `tests/query_advanced.rs`.

- **Block reader.** Accepts the text between `#+BEGIN_QUERY` / `#+END_QUERY` or a bare map/vector. `:query` may be a `[:find ...]` vector (Datalog), or a list/string (DSL, run by the simple engine). `:title`, `:collapsed?` are returned; `:result-transform`, `:view` and user `:rules` become `unsupported: <construct>` warnings and the rows are still returned.
- **Typing.** Each variable is a block, a page or a file, inferred from the attributes it appears with; a variable whose only evidence is `:block/properties` is a block, one that only appears as the value of `:block/refs` is a page; otherwise the error is `cannot tell whether ?x is a page or a block`.
- **Compilation.** Clauses are processed as: data patterns and DSL rules, function bindings, predicates, then `not` / `not-join` / `or` / `or-join` (`NOT EXISTS` / `OR EXISTS` sub-scopes). Entity variables inside `or` that nothing else binds range over their whole table. `get` on a `:block/properties` map joins `block_property_values`; property text values are compared case-insensitively (deviation 7.4.2).
- **Functions / predicates.** `= not= == < <= > >=`, `contains?` (constant set), `includes? starts-with? ends-with?`, `re-find` / `re-matches` / `re-pattern` (a `bitacora_regexp` UDF over the `regex` crate; constant patterns are validated at compile time), `missing?`, `get`, `get-else`, `str`, `identity`, `ground`, `lower-case`, `upper-case`, `+ - *`. Everything else is `unsupported: function ...`.
- **Rules.** The built-in DSL rules (`page-ref`, `block-content`, `task`, `priority`, `property`, `has-property`, `page`, `between`, `page-property`, `has-page-property`, `namespace`, `page-tags`, `all-page-tags`) are inlined through the simple-DSL leaf compiler. `:namespace` and `:alias` are direct (non-recursive) in v1.
- **Inputs.** `$`, scalar variables, `:current-page`, `:query-page`, `:current-block`, `:parent-block`, `:today`, `:yesterday`, `:tomorrow`, `:right-now-ms`, `:start-of-today-ms`, `:end-of-today-ms`, `:+-Nd|w|m|y`, `:+-Nd-start|end`, `:Nd-before[-ms]` and `"[[page]]"` strings. `:today-HHMM` is not supported.
- **Find.** Relation, `[?x ...]`, `[?x ?y]` and `?x .`; `(pull ?e pattern)` returns the whole entity (patterns with nested or reverse parts add a warning); `count`, `count-distinct`, `sum`, `min`, `max`, `avg`. Rows are `SELECT DISTINCT` (set semantics), then aggregated, ordered by the find columns. An aggregate over no rows yields one row (SQL), not an empty result (Datalog).
- **Findings for open question 7.** The patterns that matter in practice are marker/priority filters, `:block/page` + `:block/name` / `:block/journal-day` joins, `:block/refs` / `:block/path-refs`, `:block/properties` + `get`, content `includes?`, `not` / `or`, DSL rules and date inputs; all are covered. Custom rules, reverse attributes, `:result-transform` and `:view` need either rule support or a script runtime and remain open.

---

## 9. Requirements for Bitacora

1. **MUST** store the index in a per-graph SQLite file outside the graph directory by default, and treat it as a cache: delete and rebuild on schema mismatch or corruption, full reparse on `parser_version` / `config_hash` change.
2. **MUST** implement the tables and views in §3: `meta`, `files`, `pages`, `page_aliases`, `page_tags`, `blocks`, `block_page_refs`, `block_block_refs`, `block_properties`, `block_property_values`, `diagnostics`, FTS5 `blocks_fts` (unicode61) and `pages_fts` (trigram), and the views `block_path_refs`, `page_properties`, `page_property_values`, `tasks`.
3. **MUST** store the outline as pre-order `ord` + `subtree_end` + `depth` + `parent_id` per file, and derive subtrees, ancestors and path-refs from intervals.
4. **MUST** replace all rows derived from a file in a **single transaction**, upsert pages by normalized name, demote pages whose file disappeared to placeholders, and garbage-collect unreferenced placeholder pages, never built-ins.
5. **MUST** detect changes with `(size, mtime_ns)` as a prefilter and a **blake3 content hash** as the authority. Keep both in `files`.
6. **MUST** debounce watcher events (~300 ms), re-check removals after 500 ms, handle renames by hash, and fall back to a full reconcile on watcher overflow.
7. **MUST** run parsing off the writer thread (parallel, pure) and serialize all writes through one writer. Readers use separate WAL connections.
8. **MUST** prevent overwriting a file whose on-disk hash differs from the indexed hash, recognize its own writes through an expected-hash registry, and back up in Logseq's `logseq/bak` layout when it does overwrite.
9. **MUST** compile the full simple-query DSL (§7) to SQL with Logseq's result-type rule and NULL-safe negation.
10. **MUST** record duplicate page titles, duplicate block ids, invalid property keys and parse errors in `diagnostics`.
11. **SHOULD** carry block UUIDs over across reparses with the diff algorithm in §2.3, and expose `uuid_source` so the editor can write `id::` when a block is referenced.
12. **SHOULD** provide the trigram block index `blocks_fts_tri` (substring and CJK search), with a setting to disable it on very large graphs.
13. **SHOULD** run the search pipeline in §6.2 (exact title → FTS bm25 → trigram/LIKE → fuzzy titles → RRF) and generate snippets in Rust.
14. **SHOULD** store a zstd snapshot of each file's last indexed content, for carry-over diffs and "modified on disk" diffs.
15. **SHOULD** do cold builds with triggers dropped, FTS `'rebuild'` at the end and `ANALYZE` afterwards.
16. **SHOULD** compile the Datalog subset in §8 and report unsupported constructs clearly.
17. **MAY** materialize `block_path_refs` if benchmarks show the interval-join view is a bottleneck.

## 10. Open questions

1. **Index location:** keep `<app-data>/…/index.sqlite` (invisible to git and sync, but lost when the graph is moved to another machine) or use `<graph>/.bitacora/index.sqlite` (portable, Logseq ignores it, but users must gitignore it)?
2. ~~**`file_snapshots`:** is storing a compressed copy of the whole graph acceptable?~~ **Resolved (ADR-017 in [[architecture]]): no.** The merge base is kept in memory only; do not create the `file_snapshots` table.
3. **UUID policy:** is §2.3 carry-over plus "write `id::` on first reference" enough, or do we want deterministic UUIDs (UUIDv5 over file path + outline path) for blocks without `id::`, which are stable across rebuilds but not across edits?
4. **Duplicate page titles:** index the second file's blocks under the same page (current proposal: first file owns page metadata, both files' blocks are listed and a diagnostic is shown), or skip the second file like Logseq (`src/main/frontend/handler/repo.cljs:226-236`)?
5. **Tokenizers:** is `unicode61` + `trigram` the right pair, or should `blocks_fts` use a custom ICU or jieba tokenizer for CJK word segmentation instead of trigram?
6. **Property pages:** should pages created only from property **names** (ref kind 4) be hidden from "All pages" and graph view by default?
7. **Advanced queries:** which Datalog subset do real graphs need? We should sample public Logseq graphs and docs to prioritise. Is an embedded script runtime (Rhai, Steel, or a Clojure-ish interpreter) for `:result-transform` and `:view` in scope?
8. **Org-mode and whiteboards:** are `.org` pages and whiteboard `.edn` text shapes in scope for v1 indexing, or stored as `files.status = 'unsupported'`?
9. **Mtime trust:** is the `(size, mtime_ns)` prefilter safe enough on network and sync filesystems (Dropbox, iCloud, Syncthing), or should hashing be the default there, detected by filesystem type?
