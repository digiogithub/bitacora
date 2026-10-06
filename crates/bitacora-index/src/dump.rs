//! Canonical text dump of the index, for "rebuild equals incremental" comparisons
//! (AGENTS.md §8). Row ids, indexing timestamps and generated block UUIDs are left out; block
//! identity is `(file path, ord)`.

use std::fmt::Write as _;

use rusqlite::{Connection, types::ValueRef};

use crate::Error;

const QUERIES: [(&str, &str); 12] = [
    (
        "files",
        "SELECT path, kind, format, size, mtime_ns, hex(content_hash), status, error, parser_version
         FROM files ORDER BY path",
    ),
    (
        "pages",
        "SELECT p.name, CASE WHEN p.file_id IS NULL THEN NULL ELSE p.original_name END, p.uuid,
                f.path, p.format, p.is_journal, p.journal_day, n.name, p.is_builtin, p.is_whiteboard,
                p.created_at, p.updated_at, p.search_title
         FROM pages p LEFT JOIN files f ON f.id = p.file_id
         LEFT JOIN pages n ON n.id = p.namespace_parent_id
         ORDER BY p.name",
    ),
    (
        "page_aliases",
        "SELECT p.name, a.name, f.path FROM page_aliases x JOIN pages p ON p.id = x.page_id
         JOIN pages a ON a.id = x.alias_page_id JOIN files f ON f.id = x.source_file_id
         ORDER BY 1, 2, 3",
    ),
    (
        "page_tags",
        "SELECT p.name, a.name, f.path FROM page_tags x JOIN pages p ON p.id = x.page_id
         JOIN pages a ON a.id = x.tag_page_id JOIN files f ON f.id = x.source_file_id
         ORDER BY 1, 2, 3",
    ),
    (
        "blocks",
        "SELECT f.path, b.ord, CASE b.uuid_source WHEN 1 THEN b.uuid ELSE NULL END, b.uuid_source = 1,
                pg.name, par.ord, b.subtree_end, b.depth, b.sibling_idx, b.is_pre_block, b.format,
                b.content, b.title, b.search_text, b.marker, b.priority, b.scheduled, b.scheduled_raw,
                b.deadline, b.deadline_raw, b.repeated, b.collapsed, b.heading, b.created_at,
                b.updated_at, b.byte_start, b.byte_end, b.line_start, hex(b.content_hash)
         FROM blocks b JOIN files f ON f.id = b.file_id JOIN pages pg ON pg.id = b.page_id
         LEFT JOIN blocks par ON par.id = b.parent_id ORDER BY f.path, b.ord",
    ),
    (
        "block_page_refs",
        "SELECT f.path, b.ord, p.name, r.kind FROM block_page_refs r
         JOIN blocks b ON b.id = r.block_id JOIN files f ON f.id = b.file_id
         JOIN pages p ON p.id = r.page_id ORDER BY 1, 2, 3, 4",
    ),
    (
        "block_block_refs",
        "SELECT f.path, b.ord, r.target_uuid, r.kind FROM block_block_refs r
         JOIN blocks b ON b.id = r.block_id JOIN files f ON f.id = b.file_id ORDER BY 1, 2, 3, 4",
    ),
    (
        "block_properties",
        "SELECT f.path, b.ord, p.key, p.pos, p.raw_key, p.raw_value, p.value_type, p.builtin
         FROM block_properties p JOIN blocks b ON b.id = p.block_id JOIN files f ON f.id = b.file_id
         ORDER BY 1, 2, 3",
    ),
    (
        "block_property_values",
        "SELECT f.path, b.ord, v.key, v.value_norm, v.value_num, rp.name
         FROM block_property_values v JOIN blocks b ON b.id = v.block_id
         JOIN files f ON f.id = b.file_id
         LEFT JOIN pages rp ON rp.id = v.ref_page_id ORDER BY 1, 2, 3, 4",
    ),
    (
        "diagnostics",
        "SELECT f.path, d.kind, d.severity, d.line, d.message, d.data FROM diagnostics d
         JOIN files f ON f.id = d.file_id ORDER BY 1, 2, 3, 4, 5",
    ),
    (
        "fts_counts",
        "SELECT (SELECT count(*) FROM blocks), (SELECT count(*) FROM blocks_fts_docsize),
                (SELECT count(*) FROM blocks_fts_tri_docsize), (SELECT count(*) FROM pages),
                (SELECT count(*) FROM pages_fts_docsize)",
    ),
    (
        "meta",
        "SELECT key, value FROM meta
         WHERE key IN ('schema_version','parser_version','normalizer_version','config_hash')
         ORDER BY key",
    ),
];

/// Render the whole index as text, one section per table, rows in a stable order.
pub fn canonical_dump(conn: &Connection) -> Result<String, Error> {
    let mut out = String::new();
    for (name, sql) in QUERIES {
        let _ = writeln!(out, "== {name}");
        let mut stmt = conn.prepare(sql)?;
        let cols = stmt.column_count();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let cells: Vec<String> = (0..cols)
                .map(|i| match row.get_ref(i) {
                    Ok(ValueRef::Null) | Err(_) => "~".to_owned(),
                    Ok(ValueRef::Integer(n)) => n.to_string(),
                    Ok(ValueRef::Real(f)) => f.to_string(),
                    Ok(ValueRef::Text(t)) => format!("{:?}", String::from_utf8_lossy(t)),
                    Ok(ValueRef::Blob(b)) => format!("blob({})", b.len()),
                })
                .collect();
            let _ = writeln!(out, "{}", cells.join(" | "));
        }
    }
    Ok(out)
}
