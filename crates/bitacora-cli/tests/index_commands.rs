//! End-to-end tests of `bitacora-cli reindex` and `doctor` (BIT-T-0075, BIT-T-0076).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bitacora-cli"))
        .args(args)
        .output()
        .expect("run bitacora-cli")
}

fn s(p: &Path) -> &str {
    p.to_str().expect("utf-8 path")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for e in std::fs::read_dir(from).expect("read_dir") {
        let e = e.expect("entry");
        let dest = to.join(e.file_name());
        if e.file_type().expect("type").is_dir() {
            copy_dir(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), dest).expect("copy");
        }
    }
}

/// `(relative path -> content hash)` of every file under `root`.
fn tree_hash(root: &Path) -> BTreeMap<String, u64> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, u64>) {
        for e in std::fs::read_dir(dir).expect("read_dir") {
            let e = e.expect("entry");
            let p = e.path();
            if e.file_type().expect("type").is_dir() {
                walk(&p, root, out);
            } else {
                let mut h = DefaultHasher::new();
                h.write(&std::fs::read(&p).expect("read"));
                let rel = p
                    .strip_prefix(root)
                    .expect("rel")
                    .to_string_lossy()
                    .into_owned();
                out.insert(rel, h.finish());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

struct Setup {
    _tmp: tempfile::TempDir,
    graph: PathBuf,
    data: PathBuf,
}

fn setup_from(graph_files: impl FnOnce(&Path)) -> Setup {
    let tmp = tempfile::tempdir().expect("tempdir");
    let graph = tmp.path().join("graph");
    std::fs::create_dir_all(&graph).expect("graph dir");
    graph_files(&graph);
    let data = tmp.path().join("data");
    Setup {
        _tmp: tmp,
        graph,
        data,
    }
}

fn docs_graph() -> Setup {
    setup_from(|g| copy_dir(&bitacora_testkit::graph("logseq-docs"), g))
}

/// Rows per section of `canonical_dump`.
fn dump_counts(db: &Path) -> BTreeMap<String, usize> {
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open db");
    let dump = bitacora_index::dump::canonical_dump(&conn).expect("dump");
    let mut counts = BTreeMap::new();
    let mut current = String::new();
    for line in dump.lines() {
        if let Some(name) = line.strip_prefix("== ") {
            current = name.to_owned();
            counts.insert(current.clone(), 0);
        } else if let Some(n) = counts.get_mut(&current) {
            *n += 1;
        }
    }
    counts
}

fn json(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "json: {e}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

#[test]
fn reindex_counts_match_the_dump_and_the_graph_is_untouched() {
    let env = docs_graph();
    let before = tree_hash(&env.graph);
    let out = cli(&[
        "reindex",
        "--graph",
        s(&env.graph),
        "--data-dir",
        s(&env.data),
        "--json",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(tree_hash(&env.graph), before, "graph must not be written");

    let v = json(&out);
    let db = PathBuf::from(v["index"].as_str().expect("index path"));
    assert!(db.is_file());
    assert!(!db.starts_with(&env.graph));
    let counts = dump_counts(&db);
    let n = |k: &str| i64::try_from(counts[k]).expect("count");
    assert_eq!(v["files"].as_i64(), Some(n("files")));
    assert_eq!(v["pages"].as_i64(), Some(n("pages")));
    assert_eq!(v["blocks"].as_i64(), Some(n("blocks")));
    assert!(v["files"].as_i64() > Some(10));
    assert!(v["blocks"].as_i64() > v["files"].as_i64());
    assert!(v["duration_ms"].is_u64());
    assert!(v["errors"].as_array().expect("errors").is_empty());

    // A second reindex rebuilds from scratch and gives the same counts.
    let again = cli(&[
        "reindex",
        "--graph",
        s(&env.graph),
        "--data-dir",
        s(&env.data),
        "--json",
    ]);
    assert!(again.status.success());
    let v2 = json(&again);
    assert_eq!(
        (&v["files"], &v["pages"], &v["blocks"]),
        (&v2["files"], &v2["pages"], &v2["blocks"])
    );
    assert_eq!(tree_hash(&env.graph), before);
}

#[test]
fn reindex_text_output_and_missing_graph() {
    let env = setup_from(|g| {
        std::fs::create_dir_all(g.join("pages")).expect("mkdir");
        std::fs::write(g.join("pages/a.md"), "- a [[b]]\n").expect("write");
    });
    let out = cli(&[
        "reindex",
        "--graph",
        s(&env.graph),
        "--data-dir",
        s(&env.data),
    ]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for label in ["Files:", "Pages:", "Blocks:", "Duration:"] {
        assert!(text.contains(label), "{text}");
    }
    let bad = cli(&["reindex", "--graph", "/nonexistent/graph/xyz"]);
    assert_eq!(bad.status.code(), Some(1));
}

#[test]
fn doctor_is_healthy_after_reindex() {
    let env = docs_graph();
    let args = ["--graph", s(&env.graph), "--data-dir", s(&env.data)];
    assert!(cli(&[&["reindex"], &args[..]].concat()).status.success());
    let before = tree_hash(&env.graph);

    let out = cli(&[&["doctor", "--json"], &args[..]].concat());
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v = json(&out);
    assert_eq!(v["healthy"], true);
    let checks = v["checks"].as_array().expect("checks");
    assert!(checks.iter().all(|c| c["ok"] == true), "{checks:?}");
    assert_eq!(checks.len(), 5);

    let text = cli(&[&["doctor"], &args[..]].concat());
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(
        text.contains("quick_check") && text.contains("foreign_key_check"),
        "{text}"
    );
    assert!(text.contains("Status:  healthy"), "{text}");
    assert_eq!(tree_hash(&env.graph), before);
}

#[test]
fn doctor_lists_duplicate_page_diagnostics() {
    let env = setup_from(|g| {
        std::fs::create_dir_all(g.join("pages")).expect("mkdir");
        std::fs::write(g.join("pages/foo.md"), "title:: Foo\n\n- one\n").expect("write");
        std::fs::write(g.join("pages/other.md"), "title:: Foo\n\n- two\n").expect("write");
    });
    let args = ["--graph", s(&env.graph), "--data-dir", s(&env.data)];
    assert!(cli(&[&["reindex"], &args[..]].concat()).status.success());
    let out = cli(&[&["doctor"], &args[..]].concat());
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("duplicate_page (1)"), "{text}");
    assert!(text.contains("pages/other.md"), "{text}");
}

#[test]
fn doctor_exits_2_on_a_corrupt_or_missing_index() {
    let env = setup_from(|g| {
        std::fs::create_dir_all(g.join("pages")).expect("mkdir");
        std::fs::write(g.join("pages/a.md"), "- a\n").expect("write");
    });
    let args = ["--graph", s(&env.graph), "--data-dir", s(&env.data)];

    let missing = cli(&[&["doctor"], &args[..]].concat());
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stdout).contains("reindex"));

    let reindexed = cli(&[&["reindex", "--json"], &args[..]].concat());
    assert!(reindexed.status.success());
    let db = PathBuf::from(json(&reindexed)["index"].as_str().expect("index"));
    rusqlite::Connection::open(&db)
        .expect("open")
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .expect("checkpoint");
    let mut bytes = std::fs::read(&db).expect("read");
    assert!(bytes.len() > 4096);
    for b in bytes.iter_mut().skip(4096) {
        *b = 0x42;
    }
    std::fs::write(&db, bytes).expect("corrupt");

    let out = cli(&[&["doctor", "--json"], &args[..]].concat());
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert_eq!(json(&out)["healthy"], false);
    assert!(db.is_file(), "doctor must not delete the index");

    // reindex recovers.
    assert!(cli(&[&["reindex"], &args[..]].concat()).status.success());
    assert_eq!(
        cli(&[&["doctor"], &args[..]].concat()).status.code(),
        Some(0)
    );
}
