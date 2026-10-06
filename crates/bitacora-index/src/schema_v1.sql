-- =====================================================================
-- Bitacora index schema v1
-- =====================================================================
-- user_version is set by the open routine (== SCHEMA_VERSION), not by this script.

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
