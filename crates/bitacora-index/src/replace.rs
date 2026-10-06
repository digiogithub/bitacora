//! The per-file replace transaction, `delete_file`, built-in page seeding and placeholder GC
//! (`docs/design/sqlite-index-schema.md` §4.4, steps 0-10; step 11 is dropped by ADR-017).
//!
//! The functions here run inside a transaction opened by the caller (the writer thread), so a
//! batch of files can share one transaction in the cold-build fast path.

use std::collections::{HashMap, HashSet};

use bitacora_core::graph_path::GraphPath;
use bitacora_markdown::inline::namespace_parents;
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::Error;
use crate::carry::{OldBlock, assign_uuids};
use crate::normalize::{NormalizeOptions, fold};
use crate::parse::PARSER_VERSION;
use crate::parsed::{DiagnosticKind, FileFormat, PageRefName, ParsedFile, Severity};

/// Pages seeded with `is_builtin = 1`; they are never garbage-collected
/// (Logseq `deps/db/src/logseq/db/default.cljs` built-in pages plus the task markers and priorities
/// that blocks reference).
pub const BUILTIN_PAGES: [&str; 16] = [
    "TODO",
    "DOING",
    "DONE",
    "LATER",
    "NOW",
    "WAIT",
    "WAITING",
    "CANCELED",
    "CANCELLED",
    "IN-PROGRESS",
    "A",
    "B",
    "C",
    "Contents",
    "Favorites",
    "card",
];

/// Namespace of the deterministic page UUIDs (`UUIDv5(NS, name)`, design §2.1).
const NS_BITACORA_PAGE: Uuid = Uuid::from_u128(0x4249_5441_636f_7261_8000_5041_4745_0001);

/// Deterministic UUID of a page name.
#[must_use]
pub fn page_uuid(name: &str) -> String {
    Uuid::new_v5(&NS_BITACORA_PAGE, name.as_bytes())
        .hyphenated()
        .to_string()
}

/// `files.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// A page under `pages/` (or anywhere not otherwise classified).
    Page,
    /// A journal file.
    Journal,
    /// A whiteboard (`.edn` under the whiteboards directory).
    Whiteboard,
    /// An `.edn` file that is not a whiteboard.
    Config,
    /// A stylesheet.
    Css,
    /// Anything else.
    Other,
}

impl FileKind {
    /// The value stored in `files.kind`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Journal => "journal",
            Self::Whiteboard => "whiteboard",
            Self::Config => "config",
            Self::Css => "css",
            Self::Other => "other",
        }
    }

    /// Classify a graph-relative path using the configured directories.
    #[must_use]
    pub fn classify(path: &GraphPath, cfg: &bitacora_config::EffectiveConfig) -> Self {
        let p = path.as_str();
        let ext = path.extension().map(str::to_ascii_lowercase);
        match ext.as_deref() {
            Some("md" | "markdown" | "org") => {
                if p.starts_with(&format!("{}/", cfg.journals_directory())) {
                    Self::Journal
                } else {
                    Self::Page
                }
            }
            Some("edn") => {
                if p.starts_with(&format!("{}/", cfg.whiteboards_directory())) {
                    Self::Whiteboard
                } else {
                    Self::Config
                }
            }
            Some("css") => Self::Css,
            _ => Self::Other,
        }
    }
}

/// Everything the writer needs to replace one file.
#[derive(Debug, Clone)]
pub struct FileInput {
    /// Graph-relative path.
    pub path: GraphPath,
    /// `files.kind`.
    pub kind: FileKind,
    /// File size in bytes (stat).
    pub size: u64,
    /// Modification time, nanoseconds since the Unix epoch (stat).
    pub mtime_ns: i64,
    /// Birth time (ns since the epoch) when the filesystem reports it; fallback of the page's
    /// `created_at`.
    pub birth_ns: Option<i64>,
    /// The parse result.
    pub parsed: ParsedFile,
}

/// Result of replacing one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceOutcome {
    /// `files.id`.
    pub file_id: i64,
    /// Id of the page the file defines (none for files that define no page).
    pub page_id: Option<i64>,
    /// Pages whose rows or references changed (still existing after GC), sorted.
    pub page_ids_touched: Vec<i64>,
    /// UUIDs of blocks that exist now and did not before.
    pub block_uuids_added: Vec<String>,
    /// UUIDs of blocks that existed before and do not now.
    pub block_uuids_removed: Vec<String>,
    /// Final `files.status`.
    pub status: &'static str,
}

/// Result of deleting a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteOutcome {
    /// `files.id` that was removed.
    pub file_id: i64,
    /// Pages affected (still existing after GC), sorted.
    pub page_ids_touched: Vec<i64>,
    /// UUIDs of the blocks that disappeared.
    pub block_uuids_removed: Vec<String>,
}

/// Settings of the write path.
#[derive(Debug, Clone, Copy, Default)]
pub struct WriteOptions {
    /// Search normalisation (must match the parser's, for `pages.search_title`).
    pub normalize: NormalizeOptions,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Insert the built-in pages (idempotent).
pub fn seed_builtin_pages(conn: &Connection, opts: &WriteOptions) -> Result<(), Error> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO pages(name, original_name, uuid, is_builtin, search_title)
         VALUES (?1, ?2, ?3, 1, ?4)
         ON CONFLICT(name) DO UPDATE SET is_builtin = 1",
    )?;
    for original in BUILTIN_PAGES {
        let name = bitacora_core::naming::page_key(original);
        stmt.execute(params![
            name,
            original,
            page_uuid(&name),
            fold(original, opts.normalize.remove_accents)
        ])?;
    }
    Ok(())
}

fn ids(
    conn: &Connection,
    sql: &str,
    p: impl rusqlite::Params,
) -> Result<Vec<i64>, rusqlite::Error> {
    let mut stmt = conn.prepare_cached(sql)?;
    stmt.query_map(p, |r| r.get::<_, i64>(0))?.collect()
}

/// Per-replace cache of `pages.name -> pages.id`.
struct PageCache<'a> {
    conn: &'a Connection,
    opts: WriteOptions,
    ids: HashMap<String, i64>,
}

impl<'a> PageCache<'a> {
    fn new(conn: &'a Connection, opts: WriteOptions) -> Self {
        Self {
            conn,
            opts,
            ids: HashMap::new(),
        }
    }

    /// The id of `page`, creating a placeholder (and its namespace parents) when it is new.
    fn ensure(&mut self, page: &PageRefName) -> Result<i64, Error> {
        if let Some(id) = self.ids.get(&page.name) {
            return Ok(*id);
        }
        let existing: Option<i64> = self
            .conn
            .prepare_cached("SELECT id FROM pages WHERE name = ?1")?
            .query_row([&page.name], |r| r.get(0))
            .optional()?;
        let id = if let Some(id) = existing {
            id
        } else {
            let parent = self.parent_of(&page.original, &page.name)?;
            self.conn
                .prepare_cached(
                    "INSERT INTO pages(name, original_name, uuid, namespace_parent_id, search_title)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )?
                .execute(params![
                    page.name,
                    page.original,
                    page_uuid(&page.name),
                    parent,
                    fold(&page.original, self.opts.normalize.remove_accents)
                ])?;
            self.conn.last_insert_rowid()
        };
        self.ids.insert(page.name.clone(), id);
        Ok(id)
    }

    /// Immediate namespace parent of a page, ensured to exist.
    fn parent_of(&mut self, original: &str, name: &str) -> Result<Option<i64>, Error> {
        if !original.contains('/') {
            return Ok(None);
        }
        let Some(parent) = namespace_parents(original)
            .into_iter()
            .rev()
            .find(|p| bitacora_core::naming::page_key(p) != name)
        else {
            return Ok(None);
        };
        let parent = PageRefName {
            name: bitacora_core::naming::page_key(&parent),
            original: parent,
        };
        self.ensure(&parent).map(Some)
    }
}

/// Recompute `pages.search_title` = normalize(original_name + aliases) for `page_ids`.
pub(crate) fn recompute_search_titles(
    conn: &Connection,
    opts: &WriteOptions,
    page_ids: &[i64],
) -> Result<(), Error> {
    let mut title = conn.prepare_cached("SELECT original_name FROM pages WHERE id = ?1")?;
    let mut aliases = conn.prepare_cached(
        "SELECT p.original_name FROM page_aliases a JOIN pages p ON p.id = a.alias_page_id
         WHERE a.page_id = ?1 ORDER BY p.name",
    )?;
    let mut set = conn.prepare_cached("UPDATE pages SET search_title = ?2 WHERE id = ?1")?;
    for &id in page_ids {
        let Some(original) = title
            .query_row([id], |r| r.get::<_, String>(0))
            .optional()?
        else {
            continue;
        };
        let mut text = original;
        for alias in aliases.query_map([id], |r| r.get::<_, String>(0))? {
            text.push(' ');
            text.push_str(&alias?);
        }
        set.execute(params![id, fold(&text, opts.normalize.remove_accents)])?;
    }
    Ok(())
}

/// Delete candidate pages that are now unreferenced placeholders (never built-ins); a deleted
/// page's namespace parent becomes a candidate in turn. Returns the ids that remain.
fn gc_pages(conn: &Connection, candidates: &[i64]) -> Result<HashSet<i64>, Error> {
    let mut alive: HashSet<i64> = candidates.iter().copied().collect();
    let mut current: Vec<i64> = candidates.to_vec();
    let mut probe = conn.prepare_cached(
        "SELECT namespace_parent_id FROM pages
         WHERE id = ?1 AND file_id IS NULL AND is_builtin = 0
           AND NOT EXISTS (SELECT 1 FROM block_page_refs r WHERE r.page_id = pages.id)
           AND NOT EXISTS (SELECT 1 FROM block_property_values v WHERE v.ref_page_id = pages.id)
           AND NOT EXISTS (SELECT 1 FROM page_aliases a WHERE pages.id IN (a.page_id, a.alias_page_id))
           AND NOT EXISTS (SELECT 1 FROM page_tags t WHERE pages.id IN (t.page_id, t.tag_page_id))
           AND NOT EXISTS (SELECT 1 FROM pages c WHERE c.namespace_parent_id = pages.id)
           AND NOT EXISTS (SELECT 1 FROM blocks b WHERE b.page_id = pages.id)",
    )?;
    let mut delete = conn.prepare_cached("DELETE FROM pages WHERE id = ?1")?;
    while !current.is_empty() {
        let mut next = Vec::new();
        for id in current {
            let parent: Option<Option<i64>> = probe.query_row([id], |r| r.get(0)).optional()?;
            if let Some(parent) = parent {
                delete.execute([id])?;
                alive.remove(&id);
                if let Some(p) = parent {
                    alive.insert(p);
                    next.push(p);
                }
            }
        }
        current = next;
    }
    Ok(alive)
}

/// Pages touched by a file: everything it defined, referenced, aliased, tagged or owned blocks of.
fn pages_of_file(conn: &Connection, file_id: i64) -> Result<Vec<i64>, Error> {
    Ok(ids(
        conn,
        "SELECT r.page_id FROM block_page_refs r JOIN blocks b ON b.id = r.block_id WHERE b.file_id = ?1
         UNION SELECT v.ref_page_id FROM block_property_values v JOIN blocks b ON b.id = v.block_id
               WHERE b.file_id = ?1 AND v.ref_page_id IS NOT NULL
         UNION SELECT alias_page_id FROM page_aliases WHERE source_file_id = ?1
         UNION SELECT page_id FROM page_aliases WHERE source_file_id = ?1
         UNION SELECT tag_page_id FROM page_tags WHERE source_file_id = ?1
         UNION SELECT page_id FROM page_tags WHERE source_file_id = ?1
         UNION SELECT id FROM pages WHERE file_id = ?1
         UNION SELECT page_id FROM blocks WHERE file_id = ?1",
        [file_id],
    )?)
}

/// A block row of the previous index state, with the columns the retain-in-place path compares.
struct OldRow {
    id: i64,
    block: OldBlock,
    is_pre_block: bool,
    ord: i64,
    parent_id: Option<i64>,
    subtree_end: i64,
    sibling_idx: i64,
    byte_start: i64,
    byte_end: i64,
    line_start: i64,
    page_id: i64,
}

fn load_old_blocks(conn: &Connection, file_id: i64) -> Result<Vec<OldRow>, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT depth, content_hash, content, uuid, id, is_pre_block, ord, parent_id, subtree_end,
                sibling_idx, byte_start, byte_end, line_start, page_id
         FROM blocks WHERE file_id = ?1 ORDER BY ord",
    )?;
    let rows = stmt.query_map([file_id], |r| {
        let hash: Vec<u8> = r.get(1)?;
        let mut content_hash = [0u8; 16];
        let n = hash.len().min(16);
        content_hash[..n].copy_from_slice(&hash[..n]);
        Ok(OldRow {
            block: OldBlock {
                depth: r.get(0)?,
                content_hash,
                content: r.get(2)?,
                uuid: r.get(3)?,
            },
            id: r.get(4)?,
            is_pre_block: r.get::<_, i64>(5)? != 0,
            ord: r.get(6)?,
            parent_id: r.get(7)?,
            subtree_end: r.get(8)?,
            sibling_idx: r.get(9)?,
            byte_start: r.get(10)?,
            byte_end: r.get(11)?,
            line_start: r.get(12)?,
            page_id: r.get(13)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Drop everything derived from the file (design step 2) and demote pages it defined except
/// `keep_page` (step 3).
fn clear_derived(
    conn: &Connection,
    opts: &WriteOptions,
    file_id: i64,
    keep_page: Option<&str>,
    delete_blocks: bool,
) -> Result<Vec<i64>, Error> {
    if delete_blocks {
        conn.prepare_cached("DELETE FROM blocks WHERE file_id = ?1")?
            .execute([file_id])?;
    }
    conn.prepare_cached("DELETE FROM page_aliases WHERE source_file_id = ?1")?
        .execute([file_id])?;
    conn.prepare_cached("DELETE FROM page_tags WHERE source_file_id = ?1")?
        .execute([file_id])?;
    conn.prepare_cached("DELETE FROM diagnostics WHERE file_id = ?1")?
        .execute([file_id])?;
    let demoted = ids(
        conn,
        "SELECT id FROM pages WHERE file_id = ?1 AND name <> ?2",
        params![file_id, keep_page.unwrap_or("")],
    )?;
    conn.prepare_cached(
        "UPDATE pages SET file_id = NULL, format = NULL, created_at = NULL, updated_at = NULL,
                is_journal = 0, journal_day = NULL
         WHERE file_id = ?1 AND name <> ?2",
    )?
    .execute(params![file_id, keep_page.unwrap_or("")])?;
    recompute_search_titles(conn, opts, &demoted)?;
    Ok(demoted)
}

/// Replace everything derived from one file (design §4.4). Runs inside the caller's transaction.
pub fn replace_file(
    conn: &Connection,
    opts: &WriteOptions,
    input: &FileInput,
) -> Result<ReplaceOutcome, Error> {
    let parsed = &input.parsed;
    let path = input.path.as_str();
    let defines_page = matches!(parsed.page.format, FileFormat::Markdown | FileFormat::Org);

    // Step 0: remember GC candidates and the carry-over baseline.
    let old_file_id: Option<i64> = conn
        .prepare_cached("SELECT id FROM files WHERE path = ?1")?
        .query_row([path], |r| r.get(0))
        .optional()?;
    let (mut candidates, old_blocks) = match old_file_id {
        Some(id) => (pages_of_file(conn, id)?, load_old_blocks(conn, id)?),
        None => (Vec::new(), Vec::new()),
    };

    // Step 1: the file row.
    let (mut status, mut error) = ("ok", None::<String>);
    if let Some(d) = parsed
        .diagnostics
        .iter()
        .find(|d| d.kind == DiagnosticKind::ParseError)
    {
        status = "parse_error";
        error = Some(d.message.clone());
    } else if !defines_page {
        status = "unsupported";
    }
    let format = parsed.page.format.as_str();
    let file_id: i64 = conn
        .prepare_cached(
            "INSERT INTO files(path, kind, format, size, mtime_ns, content_hash, status, error,
                               parser_version, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(path) DO UPDATE SET kind = excluded.kind, format = excluded.format,
                size = excluded.size, mtime_ns = excluded.mtime_ns,
                content_hash = excluded.content_hash, status = excluded.status,
                error = excluded.error, parser_version = excluded.parser_version,
                indexed_at = excluded.indexed_at
             RETURNING id",
        )?
        .query_row(
            params![
                path,
                input.kind.as_str(),
                format,
                i64::try_from(input.size).unwrap_or(i64::MAX),
                input.mtime_ns,
                &parsed.file_hash[..],
                status,
                error,
                i64::from(PARSER_VERSION),
                now_ms()
            ],
            |r| r.get(0),
        )?;

    // Steps 2 and 3: drop derived rows, demote pages the file no longer defines.
    let defined_name = defines_page.then_some(parsed.page.name.as_str());
    let demoted = clear_derived(conn, opts, file_id, defined_name, false)?;
    candidates.extend(demoted.iter().copied());

    let mut cache = PageCache::new(conn, *opts);
    let mut page_id = None;
    let mut touched: Vec<i64> = candidates.clone();

    if defines_page {
        let page = &parsed.page;
        // Step 4: upsert the defined page.
        let parent = match page.namespace_parents.last() {
            Some(p) if p.name != page.name => Some(cache.ensure(p)?),
            _ => None,
        };
        let created = page
            .created_at
            .or_else(|| input.birth_ns.map(|n| n / 1_000_000));
        let updated = page.updated_at.or(Some(input.mtime_ns / 1_000_000));
        // Ownership of a page defined by several files is deterministic (the file with the
        // smallest path owns it) so that a rebuild gives the same result in any order.
        let existing: Option<(Option<i64>, Option<String>)> = conn
            .prepare_cached(
                "SELECT p.file_id, f.path FROM pages p LEFT JOIN files f ON f.id = p.file_id
                 WHERE p.name = ?1",
            )?
            .query_row([&page.name], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let (takes, displaced) = match &existing {
            None | Some((None, _)) => (true, None),
            Some((Some(fid), _)) if *fid == file_id => (true, None),
            Some((Some(fid), Some(owner_path))) if path < owner_path.as_str() => (true, Some(*fid)),
            Some(_) => (false, None),
        };
        let upsert = if takes {
            "INSERT INTO pages(name, original_name, uuid, file_id, format, is_journal, journal_day,
                               namespace_parent_id, created_at, updated_at, search_title)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(name) DO UPDATE SET
               original_name = excluded.original_name, format = excluded.format,
               is_journal = excluded.is_journal, journal_day = excluded.journal_day,
               namespace_parent_id = COALESCE(excluded.namespace_parent_id, pages.namespace_parent_id),
               created_at = excluded.created_at, updated_at = excluded.updated_at,
               file_id = excluded.file_id"
        } else {
            "INSERT OR IGNORE INTO pages(name, original_name, uuid, file_id, format, is_journal,
                               journal_day, namespace_parent_id, created_at, updated_at, search_title)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
        };
        conn.prepare_cached(upsert)?.execute(params![
            page.name,
            page.original_name,
            page_uuid(&page.name),
            file_id,
            page.format.as_str(),
            i64::from(page.journal_day.is_some()),
            page.journal_day,
            parent,
            created,
            updated,
            fold(&page.original_name, opts.normalize.remove_accents)
        ])?;
        if let Some(prev) = displaced {
            let msg = format!(
                "page \"{}\" is already defined by another file",
                page.original_name
            );
            conn.prepare_cached(
                "UPDATE files SET status = 'duplicate_page', error = ?2 WHERE id = ?1",
            )?
            .execute(params![prev, msg])?;
            conn.prepare_cached(
                "DELETE FROM diagnostics WHERE file_id = ?1 AND kind = 'duplicate_page'",
            )?
            .execute([prev])?;
            insert_diagnostic(
                conn,
                prev,
                "duplicate_page",
                Severity::Warning,
                None,
                &msg,
                None,
            )?;
        }
        let pid: i64 = conn
            .prepare_cached("SELECT id FROM pages WHERE name = ?1")?
            .query_row([&page.name], |r| r.get(0))?;
        cache.ids.insert(page.name.clone(), pid);
        page_id = Some(pid);
        touched.push(pid);
        let owner: Option<i64> = conn
            .prepare_cached("SELECT file_id FROM pages WHERE id = ?1")?
            .query_row([pid], |r| r.get(0))?;
        if owner != Some(file_id) {
            status = "duplicate_page";
            error = Some(format!(
                "page \"{}\" is already defined by another file",
                page.original_name
            ));
            conn.prepare_cached("UPDATE files SET status = ?2, error = ?3 WHERE id = ?1")?
                .execute(params![file_id, status, error])?;
            insert_diagnostic(
                conn,
                file_id,
                "duplicate_page",
                Severity::Warning,
                None,
                error.as_deref().unwrap_or_default(),
                None,
            )?;
        }

        // Step 5: referenced pages.
        for r in parsed
            .referenced_pages
            .iter()
            .chain(&page.namespace_parents)
            .chain(&page.aliases)
            .chain(&page.tags)
        {
            let id = cache.ensure(r)?;
            touched.push(id);
        }
    }

    // Step 6: blocks with UUIDs. Blocks identical to their predecessor keep their row (and so
    // their FTS entries, refs and properties); only positions are updated.
    let old_only: Vec<OldBlock> = old_blocks.iter().map(|r| r.block.clone()).collect();
    let mut owned_check =
        conn.prepare_cached("SELECT 1 FROM blocks WHERE uuid = ?1 AND file_id <> ?2")?;
    let assignment = assign_uuids(&old_only, &parsed.blocks, |u| {
        owned_check.exists(params![u, file_id]).unwrap_or(true)
    });
    drop(owned_check);
    let assigned = &assignment.assigned;
    let retained: Vec<Option<usize>> = if page_id.is_some() {
        parsed
            .blocks
            .iter()
            .zip(assigned)
            .zip(&assignment.equal_old)
            .map(|((b, a), eq)| {
                eq.filter(|&o| {
                    !b.is_pre_block
                        && !old_blocks[o].is_pre_block
                        && old_blocks[o].block.uuid == a.uuid
                })
            })
            .collect()
    } else {
        vec![None; parsed.blocks.len()]
    };

    if let Some(pid) = page_id {
        sync_blocks(
            conn,
            &mut cache,
            file_id,
            pid,
            parsed,
            assigned,
            &old_blocks,
            &retained,
        )?;
        // Step 8: aliases and tags.
        let mut alias = conn.prepare_cached(
            "INSERT OR IGNORE INTO page_aliases(page_id, alias_page_id, source_file_id) VALUES (?1, ?2, ?3)",
        )?;
        for a in &parsed.page.aliases {
            let aid = cache.ensure(a)?;
            if aid != pid {
                alias.execute(params![pid, aid, file_id])?;
            }
        }
        let mut tag = conn.prepare_cached(
            "INSERT OR IGNORE INTO page_tags(page_id, tag_page_id, source_file_id) VALUES (?1, ?2, ?3)",
        )?;
        for t in &parsed.page.tags {
            let tid = cache.ensure(t)?;
            tag.execute(params![pid, tid, file_id])?;
        }
        // Step 9.
        recompute_search_titles(conn, opts, &[pid])?;
    }

    if page_id.is_none() && !old_blocks.is_empty() {
        conn.prepare_cached("DELETE FROM blocks WHERE file_id = ?1")?
            .execute([file_id])?;
    }

    // Diagnostics from the parser and the cross-file `id::` clashes.
    for d in &parsed.diagnostics {
        insert_diagnostic(
            conn,
            file_id,
            d.kind.as_str(),
            d.severity,
            d.line,
            &d.message,
            None,
        )?;
    }
    for (b, a) in parsed.blocks.iter().zip(assigned) {
        if a.conflicts_with_other_file {
            let id = b.explicit_uuid.clone().unwrap_or_default();
            insert_diagnostic(
                conn,
                file_id,
                DiagnosticKind::DuplicateBlockId.as_str(),
                Severity::Warning,
                Some(b.line_start),
                &format!(
                    "id:: {id} already belongs to a block of another file; a new id was assigned in the index"
                ),
                Some(&format!("{{\"uuid\":\"{id}\"}}")),
            )?;
        }
    }

    // Step 10: GC.
    let alive = gc_pages(conn, &candidates)?;
    touched.retain(|id| alive.contains(id) || page_exists(conn, *id));
    touched.sort_unstable();
    touched.dedup();

    let old_set: HashSet<&str> = old_blocks.iter().map(|r| r.block.uuid.as_str()).collect();
    let new_set: HashSet<&str> = assigned.iter().map(|a| a.uuid.as_str()).collect();
    let added = assigned
        .iter()
        .filter(|a| !old_set.contains(a.uuid.as_str()))
        .map(|a| a.uuid.clone())
        .collect();
    let removed = old_blocks
        .iter()
        .filter(|r| !new_set.contains(r.block.uuid.as_str()))
        .map(|r| r.block.uuid.clone())
        .collect();

    Ok(ReplaceOutcome {
        file_id,
        page_id,
        page_ids_touched: touched,
        block_uuids_added: added,
        block_uuids_removed: removed,
        status,
    })
}

fn page_exists(conn: &Connection, id: i64) -> bool {
    conn.prepare_cached("SELECT 1 FROM pages WHERE id = ?1")
        .and_then(|mut s| s.exists([id]))
        .unwrap_or(false)
}

fn insert_diagnostic(
    conn: &Connection,
    file_id: i64,
    kind: &str,
    severity: Severity,
    line: Option<u32>,
    message: &str,
    data: Option<&str>,
) -> Result<(), Error> {
    conn.prepare_cached(
        "INSERT INTO diagnostics(file_id, kind, severity, line, message, data)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?
    .execute(params![file_id, kind, severity as u8, line, message, data])?;
    Ok(())
}

/// Bring the file's block rows in line with `parsed`: rows listed in `retained` (new index ->
/// old index) stay and only get new positions; every other old row is deleted; the remaining
/// new blocks are inserted with their refs and properties.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn sync_blocks(
    conn: &Connection,
    cache: &mut PageCache<'_>,
    file_id: i64,
    page_id: i64,
    parsed: &ParsedFile,
    assigned: &[crate::carry::Assigned],
    old: &[OldRow],
    retained: &[Option<usize>],
) -> Result<(), Error> {
    let kept: HashSet<usize> = retained.iter().flatten().copied().collect();
    let mut dirty = vec![false; old.len()];
    if kept.is_empty() {
        if !old.is_empty() {
            conn.prepare_cached("DELETE FROM blocks WHERE file_id = ?1")?
                .execute([file_id])?;
        }
    } else {
        let removed: HashSet<i64> = old
            .iter()
            .enumerate()
            .filter(|(i, _)| !kept.contains(i))
            .map(|(_, r)| r.id)
            .collect();
        let mut detach = conn.prepare_cached("UPDATE blocks SET parent_id = NULL WHERE id = ?1")?;
        let mut park = conn.prepare_cached("UPDATE blocks SET ord = -ord - 1 WHERE id = ?1")?;
        let new_ord: HashMap<usize, i64> = retained
            .iter()
            .enumerate()
            .filter_map(|(n, o)| o.map(|o| (o, i64::from(parsed.blocks[n].ord))))
            .collect();
        for &o in &kept {
            let row = &old[o];
            if row.parent_id.is_some_and(|p| removed.contains(&p)) {
                detach.execute([row.id])?;
                dirty[o] = true;
            }
            // A moved row leaves its ord so positions can be re-used without UNIQUE clashes.
            if new_ord.get(&o) != Some(&row.ord) {
                park.execute([row.id])?;
                dirty[o] = true;
            }
        }
        // Children before parents (descending ord) so no foreign key is left dangling.
        let mut gone: Vec<&OldRow> = old
            .iter()
            .enumerate()
            .filter(|(i, _)| !kept.contains(i))
            .map(|(_, r)| r)
            .collect();
        gone.sort_by_key(|r| std::cmp::Reverse(r.ord));
        let mut del = conn.prepare_cached("DELETE FROM blocks WHERE id = ?1")?;
        for r in gone {
            del.execute([r.id])?;
        }
    }
    let mut update = conn.prepare_cached(
        "UPDATE blocks SET ord = ?2, subtree_end = ?3, parent_id = ?4, sibling_idx = ?5,
                byte_start = ?6, byte_end = ?7, line_start = ?8, page_id = ?9
         WHERE id = ?1",
    )?;
    let mut block = conn.prepare_cached(
        "INSERT INTO blocks(uuid, uuid_source, file_id, page_id, parent_id, ord, subtree_end, depth,
            sibling_idx, is_pre_block, format, content, title, search_text, marker, priority,
            scheduled, scheduled_raw, deadline, deadline_raw, repeated, collapsed, heading,
            created_at, updated_at, byte_start, byte_end, line_start, content_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                 ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29)",
    )?;
    let mut page_ref = conn.prepare_cached(
        "INSERT OR IGNORE INTO block_page_refs(block_id, page_id, kind) VALUES (?1, ?2, ?3)",
    )?;
    let mut block_ref = conn.prepare_cached(
        "INSERT OR IGNORE INTO block_block_refs(block_id, target_uuid, kind) VALUES (?1, ?2, ?3)",
    )?;
    let mut prop = conn.prepare_cached(
        "INSERT OR IGNORE INTO block_properties(block_id, key, pos, raw_key, raw_value, value_type, builtin)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    let mut value = conn.prepare_cached(
        "INSERT OR IGNORE INTO block_property_values(block_id, key, value_norm, value_num, ref_page_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    let format = parsed.page.format.as_str().unwrap_or("markdown");
    let mut row_ids: Vec<i64> = Vec::with_capacity(parsed.blocks.len());
    for (i, (b, a)) in parsed.blocks.iter().zip(assigned).enumerate() {
        let parent_id = b.parent_ord.and_then(|o| row_ids.get(o as usize).copied());
        if let Some(o) = retained[i] {
            let row = &old[o];
            let (byte_start, byte_end) = (
                i64::try_from(b.byte_start).unwrap_or(i64::MAX),
                i64::try_from(b.byte_end).unwrap_or(i64::MAX),
            );
            if dirty[o]
                || row.parent_id != parent_id
                || row.subtree_end != i64::from(b.subtree_end)
                || row.sibling_idx != i64::from(b.sibling_idx)
                || row.byte_start != byte_start
                || row.byte_end != byte_end
                || row.line_start != i64::from(b.line_start)
                || row.page_id != page_id
            {
                update.execute(params![
                    row.id,
                    b.ord,
                    b.subtree_end,
                    parent_id,
                    b.sibling_idx,
                    byte_start,
                    byte_end,
                    b.line_start,
                    page_id
                ])?;
            }
            row_ids.push(row.id);
            continue;
        }
        block.execute(params![
            a.uuid,
            a.source,
            file_id,
            page_id,
            parent_id,
            b.ord,
            b.subtree_end,
            b.depth,
            b.sibling_idx,
            i64::from(b.is_pre_block),
            format,
            b.content,
            b.title,
            b.search_text,
            b.marker,
            b.priority,
            b.scheduled,
            b.scheduled_raw,
            b.deadline,
            b.deadline_raw,
            i64::from(b.repeated),
            i64::from(b.collapsed),
            b.heading,
            b.created_at,
            b.updated_at,
            i64::try_from(b.byte_start).unwrap_or(i64::MAX),
            i64::try_from(b.byte_end).unwrap_or(i64::MAX),
            b.line_start,
            &b.content_hash[..]
        ])?;
        let id = conn.last_insert_rowid();
        row_ids.push(id);
        for r in &b.page_refs {
            let pid = cache.ensure(&r.page)?;
            page_ref.execute(params![id, pid, r.kind as u8])?;
        }
        for r in &b.block_refs {
            block_ref.execute(params![id, r.target_uuid, r.kind as u8])?;
        }
        for p in &b.properties {
            prop.execute(params![
                id,
                p.key,
                p.pos,
                p.raw_key,
                p.raw_value,
                p.value_type as u8,
                p.builtin
            ])?;
            for v in &p.values {
                let ref_id = match &v.ref_page {
                    Some(r) => Some(cache.ensure(r)?),
                    None => None,
                };
                value.execute(params![id, p.key, v.value_norm, v.value_num, ref_id])?;
            }
        }
    }
    Ok(())
}

/// Remove a file from the index (design §4.4 `delete_file`): steps 0, 2, 3 (the page is demoted
/// unconditionally), the `files` row, then GC. Returns `None` when the path is unknown.
pub fn delete_file(
    conn: &Connection,
    opts: &WriteOptions,
    path: &str,
) -> Result<Option<DeleteOutcome>, Error> {
    let Some(file_id): Option<i64> = conn
        .prepare_cached("SELECT id FROM files WHERE path = ?1")?
        .query_row([path], |r| r.get(0))
        .optional()?
    else {
        return Ok(None);
    };
    let mut candidates = pages_of_file(conn, file_id)?;
    let removed: Vec<String> = load_old_blocks(conn, file_id)?
        .into_iter()
        .map(|r| r.block.uuid)
        .collect();
    let demoted = clear_derived(conn, opts, file_id, None, true)?;
    candidates.extend(demoted);
    conn.prepare_cached("DELETE FROM files WHERE id = ?1")?
        .execute([file_id])?;
    let alive = gc_pages(conn, &candidates)?;
    let mut touched: Vec<i64> = candidates
        .into_iter()
        .filter(|id| alive.contains(id))
        .collect();
    touched.sort_unstable();
    touched.dedup();
    Ok(Some(DeleteOutcome {
        file_id,
        page_ids_touched: touched,
        block_uuids_removed: removed,
    }))
}

/// Change a file's path in place, keeping its row (and so its blocks as the carry-over baseline).
/// A file already indexed at `to` is deleted first. Returns whether `from` existed.
pub fn rename_file_row(
    conn: &Connection,
    opts: &WriteOptions,
    from: &str,
    to: &str,
) -> Result<bool, Error> {
    let exists = conn
        .prepare_cached("SELECT 1 FROM files WHERE path = ?1")?
        .exists([from])?;
    if !exists {
        return Ok(false);
    }
    if from != to {
        delete_file(conn, opts, to)?;
        conn.prepare_cached("UPDATE files SET path = ?2 WHERE path = ?1")?
            .execute(params![from, to])?;
    }
    Ok(true)
}

/// Update `size` and `mtime_ns` only (touch, identical content). A page whose `updated_at` was
/// derived from the old mtime (no `updated-at::` property) follows the new mtime.
pub fn touch_file(conn: &Connection, path: &str, size: u64, mtime_ns: i64) -> Result<(), Error> {
    let old: Option<(i64, i64)> = conn
        .prepare_cached("SELECT id, mtime_ns FROM files WHERE path = ?1")?
        .query_row([path], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let Some((file_id, old_mtime)) = old else {
        return Ok(());
    };
    conn.prepare_cached("UPDATE files SET size = ?2, mtime_ns = ?3 WHERE id = ?1")?
        .execute(params![
            file_id,
            i64::try_from(size).unwrap_or(i64::MAX),
            mtime_ns
        ])?;
    conn.prepare_cached("UPDATE pages SET updated_at = ?3 WHERE file_id = ?1 AND updated_at = ?2")?
        .execute(params![
            file_id,
            old_mtime / 1_000_000,
            mtime_ns / 1_000_000
        ])?;
    Ok(())
}
