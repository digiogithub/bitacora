//! Startup reconcile, cold build and incremental reindex (BIT-US-0007).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use bitacora_config::EffectiveConfig;
use bitacora_core::date::Date;
use bitacora_core::graph_path::GraphPath;
use bitacora_index::dump::canonical_dump;
use bitacora_index::normalize::NormalizeOptions;
use bitacora_index::{FsChange, Index, IndexEvent, Indexer, IndexerOptions, RebuildKind};
use common::{copy_dir, count, env, env_for, strings, write};

fn start(index: &Index, graph: &std::path::Path) -> Indexer {
    Indexer::start(
        index,
        IndexerOptions::new(graph, EffectiveConfig::default()),
    )
    .expect("indexer")
}

fn gp(p: &str) -> GraphPath {
    GraphPath::new(p).expect("path")
}

fn small_graph(root: &std::path::Path) {
    write(
        root,
        "pages/Alpha.md",
        "- alpha [[Beta]]\n  id:: 6500c1a4-0000-4000-8000-000000000001\n- more #tag\n",
    );
    write(root, "pages/Beta.md", "alias:: B2\n\n- beta body\n");
    write(root, "journals/2026_10_01.md", "- old journal\n");
    write(root, "journals/2026_10_06.md", "- today\n");
    write(root, "journals/2026_09_15.md", "- september\n");
    write(root, "pages/Zed.md", "- zed\n");
}

#[test]
fn cold_build_then_unchanged_reopen_parses_nothing() {
    let env = env();
    small_graph(&env.graph);
    {
        let index = env.open();
        assert_eq!(index.outcome().rebuild, RebuildKind::FullReparse);
        let ix = start(&index, &env.graph);
        let stats = ix.reconcile().expect("reconcile");
        assert!(stats.cold_build);
        assert_eq!(stats.scanned, 6);
        assert_eq!(stats.parsed, 6);
        assert!(stats.errors.is_empty(), "{:?}", stats.errors);
        // Triggers are back and FTS is consistent after the fast path.
        assert_eq!(
            count(
                &index,
                "SELECT count(*) FROM sqlite_master WHERE type = 'trigger'"
            ),
            6
        );
        assert_eq!(
            count(
                &index,
                "SELECT count(*) FROM blocks_fts WHERE blocks_fts MATCH 'beta'"
            ),
            2
        );
        assert_eq!(
            count(
                &index,
                "SELECT count(*) FROM pages_fts WHERE pages_fts MATCH 'alias'"
            ),
            0
        );
        assert_eq!(
            count(
                &index,
                "SELECT count(*) FROM meta WHERE key = 'bulk_in_progress'"
            ),
            0
        );
        // Same process, second reconcile: nothing to do.
        let again = ix.reconcile().expect("again");
        assert_eq!((again.parsed, again.touched, again.skipped), (0, 0, 6));
        ix.shutdown();
    }
    // New process (new Index::open): versions were recorded, so no rebuild and no parse.
    let index = env.open();
    assert_eq!(index.outcome().rebuild, RebuildKind::None);
    let ix = start(&index, &env.graph);
    let stats = ix.reconcile().expect("reopen");
    assert!(!stats.cold_build);
    assert_eq!((stats.parsed, stats.touched, stats.skipped), (0, 0, 6));
}

#[test]
fn incremental_changes_touch_modify_delete_and_new_files() {
    let env = env();
    small_graph(&env.graph);
    let index = env.open();
    let ix = start(&index, &env.graph);
    ix.reconcile().expect("cold");

    // Identical content, new mtime: only size/mtime are updated.
    let same = std::fs::read(env.graph.join("pages/Zed.md")).expect("read");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(env.graph.join("pages/Zed.md"), same).expect("rewrite");
    // Changed, removed and added files.
    write(&env.graph, "pages/Alpha.md", "- alpha changed [[Gamma]]\n");
    std::fs::remove_file(env.graph.join("pages/Beta.md")).expect("rm");
    write(&env.graph, "pages/New.md", "- brand new\n");

    let stats = ix.reconcile().expect("incremental");
    assert!(!stats.cold_build);
    assert_eq!(stats.touched, 1);
    assert_eq!(stats.parsed, 2, "{stats:?}");
    assert_eq!(stats.deleted, 1);
    assert_eq!(count(&index, "SELECT count(*) FROM files"), 6);
    assert_eq!(
        count(&index, "SELECT count(*) FROM pages WHERE name = 'gamma'"),
        1
    );
    // Beta was referenced by Alpha before; now nothing references it.
    assert_eq!(
        count(&index, "SELECT count(*) FROM pages WHERE name = 'beta'"),
        0
    );
    // The explicit id vanished with the edit.
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM blocks WHERE uuid = '6500c1a4-0000-4000-8000-000000000001'"
        ),
        0
    );
}

#[test]
fn parse_priority_today_requested_journals_newest_first_then_pages() {
    let env = env();
    small_graph(&env.graph);
    let index = env.open();
    let mut opts = IndexerOptions::new(&env.graph, EffectiveConfig::default());
    opts.today = Date::new(2026, 10, 6);
    let ix = Indexer::start(&index, opts).expect("indexer");
    ix.request_priority(&gp("pages/Zed.md"));
    let stats = ix.reconcile().expect("reconcile");
    assert_eq!(
        stats.parse_order,
        [
            "journals/2026_10_06.md", // today
            "pages/Zed.md",           // requested by the UI
            "journals/2026_10_01.md", // journals newest first
            "journals/2026_09_15.md",
            "pages/Alpha.md", // then pages
            "pages/Beta.md",
        ]
    );
}

#[test]
fn watcher_events_modify_delete_rename_and_overflow() {
    let env = env();
    small_graph(&env.graph);
    let index = env.open();
    let ix = start(&index, &env.graph);
    ix.reconcile().expect("cold");
    let rx = ix.subscribe();

    write(&env.graph, "pages/Zed.md", "- zed edited\n");
    ix.handle(&FsChange::Modified(gp("pages/Zed.md")))
        .expect("modified");
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM blocks WHERE content = 'zed edited'"
        ),
        1
    );

    // Rename with identical content keeps block UUIDs.
    let before = strings(
        &index,
        "SELECT uuid FROM blocks b JOIN files f ON f.id = b.file_id WHERE f.path = 'pages/Alpha.md' ORDER BY ord",
    );
    std::fs::rename(
        env.graph.join("pages/Alpha.md"),
        env.graph.join("pages/Omega.md"),
    )
    .expect("mv");
    ix.handle(&FsChange::Renamed {
        from: gp("pages/Alpha.md"),
        to: gp("pages/Omega.md"),
    })
    .expect("renamed");
    let after = strings(
        &index,
        "SELECT uuid FROM blocks b JOIN files f ON f.id = b.file_id WHERE f.path = 'pages/Omega.md' ORDER BY ord",
    );
    assert_eq!(before, after);
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM files WHERE path = 'pages/Alpha.md'"
        ),
        0
    );
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM pages WHERE name = 'omega' AND file_id IS NOT NULL"
        ),
        1
    );

    std::fs::remove_file(env.graph.join("pages/Zed.md")).expect("rm");
    ix.handle(&FsChange::Deleted(gp("pages/Zed.md")))
        .expect("deleted");
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM files WHERE path = 'pages/Zed.md'"
        ),
        0
    );

    // Events were emitted for each change.
    let kinds: Vec<&'static str> = rx
        .try_iter()
        .map(|e| match e {
            IndexEvent::FileReplaced { .. } => "replaced",
            IndexEvent::FileDeleted { .. } => "deleted",
            IndexEvent::FileRenamed { .. } => "renamed",
            IndexEvent::BulkFinished => "bulk",
        })
        .collect();
    assert_eq!(kinds, ["replaced", "renamed", "replaced", "deleted"]);

    // Changes made without events are found by the overflow full reconcile.
    write(&env.graph, "pages/Late.md", "- late\n");
    let stats = ix
        .handle(&FsChange::Overflow)
        .expect("overflow")
        .expect("stats");
    assert_eq!(stats.parsed, 1);
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM files WHERE path = 'pages/Late.md'"
        ),
        1
    );

    // Ignored and non-page paths are left alone.
    write(&env.graph, "pages/skip.json", "{}");
    ix.handle(&FsChange::Modified(gp("pages/skip.json")))
        .expect("json");
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM files WHERE path = 'pages/skip.json'"
        ),
        0
    );
}

#[test]
fn parser_version_change_forces_full_reparse_and_fts_only_renormalizes() {
    let env = env();
    write(&env.graph, "pages/Cafe.md", "- Éclair café\n");
    {
        let index = env.open();
        let ix = start(&index, &env.graph);
        ix.reconcile().expect("cold");
        assert_eq!(
            strings(&index, "SELECT search_text FROM blocks"),
            ["eclair cafe"]
        );
    }
    // Normalizer bump alone: FtsOnly, files are not reparsed but search_text is recomputed.
    {
        let cfg = EffectiveConfig::default();
        let loc = bitacora_index::IndexLocation::in_data_dir(&env.data, &env.graph).expect("loc");
        let mut o = bitacora_index::OpenOptions::for_config(&env.graph, &cfg);
        o.normalizer_version += 1;
        let index = Index::open(loc, o).expect("open");
        assert_eq!(index.outcome().rebuild, RebuildKind::FtsOnly);
        let mut opts = IndexerOptions::new(&env.graph, cfg);
        opts.normalize = NormalizeOptions {
            remove_accents: false,
            ..NormalizeOptions::default()
        };
        let ix = Indexer::start(&index, opts).expect("indexer");
        let stats = ix.reconcile().expect("reconcile");
        assert_eq!(stats.parsed, 0);
        assert_eq!(
            strings(&index, "SELECT search_text FROM blocks"),
            ["éclair café"]
        );
        assert_eq!(
            count(
                &index,
                "SELECT count(*) FROM blocks_fts WHERE blocks_fts MATCH 'café'"
            ),
            1
        );
        ix.shutdown();
        assert_eq!(index.outcome().rebuild, RebuildKind::FtsOnly);
    }
    // Parser version bump: everything is reparsed from nothing.
    {
        let cfg = EffectiveConfig::default();
        let loc = bitacora_index::IndexLocation::in_data_dir(&env.data, &env.graph).expect("loc");
        let mut o = bitacora_index::OpenOptions::for_config(&env.graph, &cfg);
        o.parser_version += 1;
        o.normalizer_version += 1;
        let index = Index::open(loc, o).expect("open");
        assert_eq!(index.outcome().rebuild, RebuildKind::FullReparse);
        let ix = start(&index, &env.graph);
        let stats = ix.reconcile().expect("reconcile");
        assert!(stats.cold_build);
        assert_eq!(stats.parsed, 1);
        assert_eq!(count(&index, "SELECT count(*) FROM files"), 1);
    }
}

#[test]
fn interrupted_cold_build_is_redone() {
    let env = env();
    small_graph(&env.graph);
    let index = env.open();
    {
        let ix = start(&index, &env.graph);
        ix.reconcile().expect("cold");
        ix.shutdown();
    }
    // Simulate a crash in the middle of a bulk build: flag set, triggers gone.
    {
        let w = index.take_writer().expect("writer");
        w.conn()
            .execute_batch(
                "INSERT INTO meta(key, value) VALUES ('bulk_in_progress', '1');
                 DROP TRIGGER blocks_ai; DELETE FROM blocks WHERE ord > 1;",
            )
            .expect("sabotage");
    }
    let ix = start(&index, &env.graph);
    let stats = ix.reconcile().expect("recover");
    assert!(stats.cold_build);
    assert_eq!(stats.parsed, 6);
    assert_eq!(
        count(
            &index,
            "SELECT count(*) FROM sqlite_master WHERE type = 'trigger'"
        ),
        6
    );
    assert_eq!(
        count(&index, "SELECT count(*) FROM blocks"),
        count(&index, "SELECT count(*) FROM blocks_fts_docsize")
    );
}

/// Cold build of every fixture graph equals building the same graph file by file through
/// watcher events (rebuild == incremental) and leaves a consistent FTS index.
#[test]
fn fixtures_cold_build_equals_incremental() {
    for name in bitacora_testkit::graph_names() {
        let src = bitacora_testkit::graph(&name);

        let cold_env = env_for(src.clone());
        let cold_index = cold_env.open();
        let cold = start(&cold_index, &src);
        let stats = cold.reconcile().expect("cold");
        assert!(stats.errors.is_empty(), "{name}: {:?}", stats.errors);
        let cold_dump = canonical_dump(&cold_index.reader().expect("r")).expect("dump");

        let inc_env = env_for(src.clone());
        let inc_index = inc_env.open();
        let inc = start(&inc_index, &src);
        // Start from an empty reconcile (nothing on disk is "known"), then feed files one by
        // one in reverse order to also exercise order independence.
        inc.writer().begin_bulk(false).expect("bulk");
        inc.writer().end_bulk().expect("end bulk");
        let mut paths: Vec<String> =
            strings(&cold_index, "SELECT path FROM files ORDER BY path DESC");
        paths.sort();
        paths.reverse();
        for p in &paths {
            inc.update_path(&gp(p)).expect("update");
        }
        let inc_dump = canonical_dump(&inc_index.reader().expect("r")).expect("dump");
        similar_asserts(&name, &cold_dump, &inc_dump);

        let r = rusqlite::Connection::open(cold_index.location().db_path()).expect("rw");
        for fts in ["blocks_fts", "blocks_fts_tri", "pages_fts"] {
            r.execute_batch(&format!(
                "INSERT INTO {fts}({fts}, rank) VALUES('integrity-check', 1)"
            ))
            .unwrap_or_else(|e| panic!("{name}: {fts} integrity: {e}"));
        }
    }
}

fn similar_asserts(name: &str, a: &str, b: &str) {
    if a != b {
        let (la, lb): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
        let first = la
            .iter()
            .zip(&lb)
            .position(|(x, y)| x != y)
            .unwrap_or(la.len().min(lb.len()));
        panic!(
            "{name}: cold and incremental dumps differ at line {first}\n cold: {:?}\n incr: {:?}",
            la.get(first),
            lb.get(first)
        );
    }
}

#[test]
fn copy_dir_helper_smoke() {
    let env = env();
    write(&env.graph, "pages/a.md", "- x\n");
    let dst = env.tmp.path().join("copy");
    copy_dir(&env.graph, &dst);
    assert!(dst.join("pages/a.md").is_file());
}
