//! Open/validate lifecycle tests for the index database (BIT-US-0004).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::Write as _;
use std::path::PathBuf;

use bitacora_index::schema::SCHEMA_VERSION;
use bitacora_index::{
    Error, Index, IndexLocation, OpenOptions, RebuildKind, RecreateReason, graph_id,
};

struct Env {
    _tmp: tempfile::TempDir,
    graph: PathBuf,
    data: PathBuf,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().expect("tempdir");
    let graph = tmp.path().join("graph");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&graph).expect("graph dir");
    Env {
        graph,
        data,
        _tmp: tmp,
    }
}

impl Env {
    fn location(&self) -> IndexLocation {
        IndexLocation::in_data_dir(&self.data, &self.graph).expect("location")
    }
    fn open(&self, edit: impl FnOnce(&mut OpenOptions)) -> Index {
        let mut o = OpenOptions::new(&self.graph, "cfg-a");
        edit(&mut o);
        Index::open(self.location(), o).expect("open")
    }
    /// Open and persist the expected versions, as a finished reindex would.
    fn open_and_record(&self, edit: impl FnOnce(&mut OpenOptions)) {
        let idx = self.open(edit);
        idx.take_writer()
            .expect("writer")
            .record_versions()
            .expect("record");
    }
}

#[test]
fn graph_id_is_16_hex_stable_and_per_graph() {
    let env = env();
    let a = graph_id(&env.graph).expect("id");
    assert_eq!(a.len(), 16);
    assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(a, graph_id(&env.graph).expect("again"));
    assert_ne!(a, graph_id(&env.data).expect("other"));
}

#[test]
fn layout_matches_adr_005_and_rejects_paths_inside_graph() {
    let env = env();
    let loc = env.location();
    assert_eq!(
        loc.db_path(),
        env.data
            .join("bitacora")
            .join("graphs")
            .join(loc.graph_id())
            .join("index.sqlite")
    );
    let err = IndexLocation::in_data_dir(&env.graph.join(".data"), &env.graph).unwrap_err();
    assert!(matches!(err, Error::InsideGraph(_)));
}

#[test]
fn platform_default_location_resolves() {
    let env = env();
    // Some CI sandboxes have no home directory; only assert the shape when it resolves.
    if let Ok(loc) = IndexLocation::for_graph(&env.graph) {
        let p = loc.db_path();
        assert!(p.ends_with(format!("bitacora/graphs/{}/index.sqlite", loc.graph_id())));
        assert!(!p.starts_with(&env.graph));
    }
}

#[test]
fn open_fresh_creates_db_outside_graph() {
    let env = env();
    let idx = env.open(|_| {});
    let out = idx.outcome();
    assert!(out.created);
    assert_eq!(out.recreated, None);
    assert_eq!(out.rebuild, RebuildKind::FullReparse);
    assert!(idx.location().db_path().exists());
    assert_eq!(std::fs::read_dir(&env.graph).expect("ls").count(), 0);
}

#[test]
fn schema_objects_exist() {
    let env = env();
    let idx = env.open(|_| {});
    let r = idx.reader().expect("reader");
    let count = |kind: &str, name: &str| -> i64 {
        r.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
            [kind, name],
            |row| row.get(0),
        )
        .expect("count")
    };
    for t in [
        "meta",
        "files",
        "pages",
        "page_aliases",
        "page_tags",
        "blocks",
        "block_page_refs",
        "block_block_refs",
        "block_properties",
        "block_property_values",
        "diagnostics",
        "blocks_fts",
        "blocks_fts_tri",
        "pages_fts",
    ] {
        assert_eq!(count("table", t), 1, "table {t}");
    }
    for t in [
        "blocks_ai",
        "blocks_ad",
        "blocks_au",
        "pages_ai",
        "pages_ad",
        "pages_au",
    ] {
        assert_eq!(count("trigger", t), 1, "trigger {t}");
    }
    for v in [
        "block_path_refs",
        "page_properties",
        "page_property_values",
        "tasks",
    ] {
        assert_eq!(count("view", v), 1, "view {v}");
    }
    assert_eq!(count("table", "file_snapshots"), 0, "dropped by ADR-017");
    let uv: i64 = r
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("uv");
    assert_eq!(uv, SCHEMA_VERSION);
}

#[test]
fn pragmas_applied() {
    let env = env();
    let idx = env.open(|_| {});
    let w = idx.take_writer().expect("writer");
    let mode: String = w
        .conn()
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .expect("mode");
    assert_eq!(mode, "wal");
    let fk: i64 = w
        .conn()
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .expect("fk");
    assert_eq!(fk, 1);
    let r = idx.reader().expect("reader");
    let q: i64 = r
        .query_row("PRAGMA query_only", [], |row| row.get(0))
        .expect("qo");
    assert_eq!(q, 1);
    assert!(r.execute("DELETE FROM meta", []).is_err());
}

#[test]
fn reopen_keeps_data_and_needs_no_rebuild() {
    let env = env();
    {
        let idx = env.open(|_| {});
        let mut w = idx.take_writer().expect("writer");
        w.record_versions().expect("record");
        w.conn()
            .execute("INSERT INTO meta(key, value) VALUES ('marker', 'x')", [])
            .expect("insert");
    }
    let idx = env.open(|_| {});
    let out = idx.outcome();
    assert!(!out.created);
    assert_eq!(out.recreated, None);
    assert_eq!(out.rebuild, RebuildKind::None);
    assert_eq!(
        out.stored.as_ref().map(|s| s.config_hash.as_str()),
        Some("cfg-a")
    );
    let v: String = idx
        .reader()
        .expect("reader")
        .query_row("SELECT value FROM meta WHERE key = 'marker'", [], |r| {
            r.get(0)
        })
        .expect("marker");
    assert_eq!(v, "x");
}

#[test]
fn schema_version_bump_recreates_db() {
    let env = env();
    {
        let idx = env.open(|_| {});
        let w = idx.take_writer().expect("writer");
        w.conn()
            .execute("INSERT INTO meta(key, value) VALUES ('marker', 'x')", [])
            .expect("insert");
        w.conn()
            .execute_batch(&format!("PRAGMA user_version = {};", SCHEMA_VERSION + 1))
            .expect("bump");
    }
    let idx = env.open(|_| {});
    let out = idx.outcome();
    assert!(out.created);
    assert_eq!(out.recreated, Some(RecreateReason::SchemaVersionMismatch));
    assert_eq!(out.rebuild, RebuildKind::FullReparse);
    let n: i64 = idx
        .reader()
        .expect("reader")
        .query_row("SELECT count(*) FROM meta WHERE key = 'marker'", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(n, 0);
}

#[test]
fn garbage_file_is_recovered() {
    let env = env();
    let loc = env.location();
    std::fs::create_dir_all(loc.dir()).expect("dir");
    let mut f = std::fs::File::create(loc.db_path()).expect("create");
    f.write_all(&[0xAB; 8192]).expect("write");
    drop(f);
    let idx = env.open(|_| {});
    let out = idx.outcome();
    assert!(out.created);
    assert_eq!(out.recreated, Some(RecreateReason::Corrupt));
    assert!(idx.reader().is_ok());
}

#[test]
fn damaged_database_pages_are_recovered() {
    let env = env();
    let path = {
        let idx = env.open(|_| {});
        let mut w = idx.take_writer().expect("writer");
        w.record_versions().expect("record");
        w.conn()
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .expect("checkpoint");
        idx.location().db_path()
    };
    let mut bytes = std::fs::read(&path).expect("read");
    assert!(bytes.len() > 8192);
    for b in &mut bytes[4096..] {
        *b = 0xFF;
    }
    std::fs::write(&path, bytes).expect("write");
    let idx = env.open(|_| {});
    assert!(idx.outcome().created);
    assert_eq!(idx.outcome().recreated, Some(RecreateReason::Corrupt));
}

#[test]
fn empty_file_is_initialized_in_place() {
    let env = env();
    let loc = env.location();
    std::fs::create_dir_all(loc.dir()).expect("dir");
    std::fs::write(loc.db_path(), b"").expect("empty");
    let idx = env.open(|_| {});
    assert!(idx.outcome().created);
    assert_eq!(idx.outcome().recreated, None);
}

#[test]
fn config_hash_change_flags_full_reparse_until_recorded() {
    let env = env();
    env.open_and_record(|_| {});
    let idx = env.open(|o| o.config_hash = "cfg-b".into());
    assert!(!idx.outcome().created);
    assert_eq!(idx.outcome().rebuild, RebuildKind::FullReparse);
    drop(idx);
    // Not persisted until record_versions, so a crash mid-rebuild re-flags it.
    let idx = env.open(|o| o.config_hash = "cfg-b".into());
    assert_eq!(idx.outcome().rebuild, RebuildKind::FullReparse);
    idx.take_writer()
        .expect("writer")
        .record_versions()
        .expect("record");
    drop(idx);
    let idx = env.open(|o| o.config_hash = "cfg-b".into());
    assert_eq!(idx.outcome().rebuild, RebuildKind::None);
}

#[test]
fn parser_version_change_flags_full_reparse() {
    let env = env();
    env.open_and_record(|_| {});
    let idx = env.open(|o| o.parser_version += 1);
    assert_eq!(idx.outcome().rebuild, RebuildKind::FullReparse);
}

#[test]
fn normalizer_version_change_flags_fts_only() {
    let env = env();
    env.open_and_record(|_| {});
    let idx = env.open(|o| o.normalizer_version += 1);
    assert!(!idx.outcome().created);
    assert_eq!(idx.outcome().rebuild, RebuildKind::FtsOnly);
}

#[test]
fn fts5_word_and_trigram_available() {
    let env = env();
    let idx = env.open(|_| {});
    let w = idx.take_writer().expect("writer");
    let c = w.conn();
    c.execute_batch(
        "INSERT INTO files(id, path, kind, size, mtime_ns, content_hash, parser_version, indexed_at)
           VALUES (1, 'pages/a.md', 'page', 0, 0, x'00', 1, 0);
         INSERT INTO pages(id, name, original_name, uuid, file_id, search_title)
           VALUES (1, 'a', 'A', 'u1', 1, 'Alpha Page');
         INSERT INTO blocks(id, uuid, uuid_source, file_id, page_id, ord, subtree_end, depth,
                            sibling_idx, format, content, title, search_text, byte_start,
                            byte_end, line_start, content_hash)
           VALUES (1, 'b1', 0, 1, 1, 1, 1, 1, 0, 'markdown', 'c', 't', 'hello wonderful world',
                   0, 1, 1, x'00');",
    )
    .expect("seed");
    let count = |sql: &str| -> i64 { c.query_row(sql, [], |r| r.get(0)).expect(sql) };
    assert_eq!(
        count("SELECT count(*) FROM blocks_fts WHERE blocks_fts MATCH 'wond*'"),
        1
    );
    assert_eq!(
        count("SELECT count(*) FROM blocks_fts_tri WHERE blocks_fts_tri MATCH 'nderf'"),
        1
    );
    assert_eq!(
        count("SELECT count(*) FROM pages_fts WHERE pages_fts MATCH 'lpha'"),
        1
    );
    // The delete trigger keeps the external-content FTS in sync.
    c.execute("DELETE FROM blocks WHERE id = 1", [])
        .expect("del");
    assert_eq!(
        count("SELECT count(*) FROM blocks_fts_tri WHERE blocks_fts_tri MATCH 'nderf'"),
        0
    );
}

#[test]
fn single_writer_ownership() {
    let env = env();
    let idx = env.open(|_| {});
    let w = idx.take_writer().expect("first");
    assert!(matches!(idx.take_writer(), Err(Error::WriterTaken)));
    drop(w);
    assert!(idx.take_writer().is_ok());
}

#[test]
fn reader_pool_reuses_and_bounds_connections() {
    let env = env();
    let idx = env.open(|o| o.reader_pool_size = 2);
    let a = idx.reader().expect("a");
    let b = idx.reader().expect("b");
    assert_eq!(idx.readers().open_connections(), 2);
    drop(a);
    let _c = idx.reader().expect("c reuses a");
    assert_eq!(idx.readers().open_connections(), 2);
    drop(b);
}

#[test]
fn reader_sees_committed_writes_while_writer_is_open() {
    let env = env();
    let idx = env.open(|_| {});
    let w = idx.take_writer().expect("writer");
    w.conn()
        .execute("INSERT INTO meta(key, value) VALUES ('k', 'v')", [])
        .expect("insert");
    let v: String = idx
        .reader()
        .expect("reader")
        .query_row("SELECT value FROM meta WHERE key = 'k'", [], |r| r.get(0))
        .expect("read");
    assert_eq!(v, "v");
}
