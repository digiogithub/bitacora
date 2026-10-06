//! Diagnostics read API and the doctor report (BIT-SP-0003.R11; BIT-T-0074).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod read_common;

use bitacora_index::{DiagnosticFilter, DiagnosticKind, inspect_index};
use read_common::indexed;

const SHARED: &str = "6650a1b2-0000-4000-8000-000000000001";

#[test]
fn kind_names_round_trip() {
    for k in [
        DiagnosticKind::ParseError,
        DiagnosticKind::DuplicatePage,
        DiagnosticKind::DuplicateBlockId,
        DiagnosticKind::InvalidProperty,
        DiagnosticKind::CaseConflict,
        DiagnosticKind::TooLarge,
        DiagnosticKind::Unsupported,
    ] {
        assert_eq!(DiagnosticKind::from_name(k.as_str()), Some(k));
    }
    assert_eq!(DiagnosticKind::from_name("nope"), None);
    let json = serde_json::to_string(&DiagnosticKind::DuplicatePage).expect("json");
    assert_eq!(json, "\"duplicate_page\"");
}

#[test]
fn duplicate_title_and_duplicate_block_id_are_readable() {
    let fx = indexed(&[
        ("pages/foo.md", "title:: Foo\n\n- one\n"),
        ("pages/other.md", "title:: Foo\n\n- two\n"),
        ("pages/a.md", &format!("- first\n  id:: {SHARED}\n")),
        ("pages/b.md", &format!("- second\n  id:: {SHARED}\n")),
    ]);
    let r = &fx.reader;
    let dup_pages = r
        .diagnostics(&DiagnosticFilter {
            kind: Some(DiagnosticKind::DuplicatePage),
            ..DiagnosticFilter::default()
        })
        .expect("diag");
    assert_eq!(dup_pages.len(), 1);
    assert_eq!(dup_pages[0].file.as_deref(), Some("pages/other.md"));
    assert!(dup_pages[0].severity >= 1);

    let dup_ids = r
        .diagnostics(&DiagnosticFilter {
            kind: Some(DiagnosticKind::DuplicateBlockId),
            ..DiagnosticFilter::default()
        })
        .expect("diag");
    assert_eq!(dup_ids.len(), 1);
    assert_eq!(dup_ids[0].file.as_deref(), Some("pages/b.md"));
    assert!(dup_ids[0].line.is_some());

    let by_file = r
        .diagnostics(&DiagnosticFilter {
            file: Some("pages/other.md".into()),
            ..DiagnosticFilter::default()
        })
        .expect("diag");
    assert_eq!(by_file.len(), 1);
    let none = r
        .diagnostics(&DiagnosticFilter {
            min_severity: Some(2),
            ..DiagnosticFilter::default()
        })
        .expect("diag");
    assert!(none.iter().all(|d| d.severity >= 2));

    let counts = r.diagnostic_counts().expect("counts");
    let count = |k| counts.iter().find(|c| c.kind == k).map(|c| c.count);
    assert_eq!(count(DiagnosticKind::DuplicatePage), Some(1));
    assert_eq!(count(DiagnosticKind::DuplicateBlockId), Some(1));
}

#[test]
fn doctor_report_is_healthy_on_a_fresh_index_and_lists_diagnostics() {
    let fx = indexed(&[
        ("pages/foo.md", "title:: Foo\n\n- one\n"),
        ("pages/other.md", "title:: Foo\n\n- two\n"),
    ]);
    let report = inspect_index(&fx.index.location().db_path());
    assert!(report.healthy(), "{:?}", report.checks);
    let names: Vec<&str> = report.checks.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "quick_check",
            "foreign_key_check",
            "fts:blocks_fts",
            "fts:blocks_fts_tri",
            "fts:pages_fts"
        ]
    );
    assert!(report.stats.expect("stats").blocks >= 2);
    assert!(report.meta.iter().any(|(k, _)| k == "schema_version"));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::DuplicatePage)
    );
}

#[test]
fn doctor_reports_a_corrupt_file_without_repairing_it() {
    let fx = indexed(&[("pages/a.md", "- a\n")]);
    let path = fx.index.location().db_path();
    let read_common::Fixture { env, index, reader } = fx;
    // Move the WAL into the main file, then close every connection.
    rusqlite::Connection::open(&path)
        .expect("open")
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .expect("checkpoint");
    drop((index, reader));
    // Keep the header so the file opens, damage the rest.
    let mut bytes = std::fs::read(&path).expect("read db");
    assert!(bytes.len() > 4096, "database should span several pages");
    let keep = 4096;
    for b in bytes.iter_mut().skip(keep) {
        *b = 0x42;
    }
    std::fs::write(&path, &bytes).expect("write db");
    let before = std::fs::read(&path).expect("read db");
    let report = inspect_index(&path);
    assert!(!report.healthy(), "{:?}", report.checks);
    assert_eq!(
        std::fs::read(&path).expect("read db"),
        before,
        "file untouched"
    );

    let missing = inspect_index(&path.with_file_name("absent.sqlite3"));
    assert!(!missing.exists && !missing.healthy());
    drop(env);
}
