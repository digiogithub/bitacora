//! End-to-end behaviour of a [`Session`] on a temp graph: queue -> disk -> index, watcher echo
//! suppression, external changes, the sync writer adapter, MCP and shutdown.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::io::{Read as _, Write as _};
use std::time::Duration;

use bitacora_core::editor::Cmd;
use bitacora_core::queue::{QueueEvent, Request, Source};
use bitacora_runtime::{McpOptions, QueueGraphWriter, RuntimeEvent, Session};
use bitacora_sync::writer::{FileChange, GraphWriter, WriterError};
use common::{PAGE, blocks, config, drain, graph_with, index_has, wait_for};

const LONG: &str = "- Alpha block with some words in it\n- Beta block stays as it is\n";

#[test]
fn open_reconciles_the_index() {
    let tmp = tempfile::tempdir().unwrap();
    let graph = graph_with(tmp.path(), &[("pages/p.md", PAGE)]);
    let s = Session::open(config(&graph, &tmp.path().join("data"))).unwrap();
    assert!(s.open_stats().unwrap().parsed >= 1);
    assert!(index_has(&s, "Beta"));
    assert!(s.shutdown(Duration::from_secs(10)).is_clean());
}

#[test]
fn queue_edit_is_written_atomically_indexed_and_not_echoed() {
    let tmp = tempfile::tempdir().unwrap();
    let graph = graph_with(tmp.path(), &[("pages/p.md", PAGE)]);
    let s = Session::open(config(&graph, &tmp.path().join("data"))).unwrap();
    let events = s.subscribe();
    let key = s.open_page("pages/p.md").unwrap();
    let id = blocks(&s, &key)[0].0;

    s.queue()
        .run(
            Source::Ui,
            "edit",
            Cmd::SetText {
                id,
                text: "Alpha edited".into(),
            },
        )
        .unwrap();
    let report = s.queue().flush(Source::Ui).unwrap();
    assert_eq!(report.written.len(), 1);

    // On disk, byte for byte, without temp files left behind.
    assert_eq!(
        std::fs::read_to_string(graph.join("pages/p.md")).unwrap(),
        "- Alpha edited\n- Beta\n- Gamma\n"
    );
    let leftovers: Vec<_> = std::fs::read_dir(graph.join("pages"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".bitacora-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");

    // Index follows the write.
    wait_for("index update", Duration::from_secs(10), || {
        index_has(&s, "Alpha edited").then_some(())
    });

    // The watcher must stay silent about our own write (debounce 50 ms, poll 300 ms).
    std::thread::sleep(Duration::from_millis(1500));
    let seen = drain(&events);
    assert!(
        !seen.iter().any(|e| matches!(
            e,
            RuntimeEvent::ExternalChange { .. }
                | RuntimeEvent::ExternalChangeWhileDirty { .. }
                | RuntimeEvent::Queue(QueueEvent::PageReloaded(_))
        )),
        "own write echoed: {seen:?}"
    );
    assert!(
        seen.iter()
            .any(|e| matches!(e, RuntimeEvent::Queue(QueueEvent::Flushed(_))))
    );
    assert!(s.shutdown(Duration::from_secs(10)).is_clean());
}

#[test]
fn external_write_updates_index_and_reloads_core() {
    let tmp = tempfile::tempdir().unwrap();
    let graph = graph_with(tmp.path(), &[("pages/p.md", PAGE)]);
    let s = Session::open(config(&graph, &tmp.path().join("data"))).unwrap();
    let events = s.subscribe();
    let key = s.open_page("pages/p.md").unwrap();
    let old_ids = blocks(&s, &key);

    std::fs::write(
        graph.join("pages/p.md"),
        "- Alpha\n- Beta changed outside\n- Gamma\n",
    )
    .unwrap();

    wait_for("external change", Duration::from_secs(10), || {
        events.try_iter().find_map(|e| match e {
            RuntimeEvent::ExternalChange {
                path,
                reloaded: true,
            } if path == "pages/p.md" => Some(()),
            _ => None,
        })
    });
    let now = blocks(&s, &key);
    assert_eq!(now[1].1, "Beta changed outside");
    assert_eq!(now[0].0, old_ids[0].0, "untouched blocks keep their ids");
    assert_eq!(now[2].0, old_ids[2].0, "untouched blocks keep their ids");
    wait_for("index", Duration::from_secs(10), || {
        index_has(&s, "changed outside").then_some(())
    });
    assert!(s.shutdown(Duration::from_secs(10)).is_clean());
}

#[test]
fn new_and_deleted_external_files_reach_the_index() {
    let tmp = tempfile::tempdir().unwrap();
    let graph = graph_with(tmp.path(), &[("pages/p.md", PAGE)]);
    let s = Session::open(config(&graph, &tmp.path().join("data"))).unwrap();
    std::fs::write(graph.join("pages/q.md"), "- Brand new\n").unwrap();
    wait_for("new file indexed", Duration::from_secs(10), || {
        index_has(&s, "Brand new").then_some(())
    });
    std::fs::remove_file(graph.join("pages/q.md")).unwrap();
    wait_for("deleted file dropped", Duration::from_secs(10), || {
        (!index_has(&s, "Brand new")).then_some(())
    });
    drop(s);
}

#[test]
fn external_change_to_a_dirty_page_never_overwrites_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let graph = graph_with(tmp.path(), &[("pages/p.md", LONG)]);
    let mut cfg = config(&graph, &tmp.path().join("data"));
    cfg.debounce = None; // only explicit flushes write
    let s = Session::open(cfg).unwrap();
    let events = s.subscribe();
    let key = s.open_page("pages/p.md").unwrap();
    let id = blocks(&s, &key)[0].0;
    s.queue()
        .run(
            Source::Ui,
            "edit",
            Cmd::SetText {
                id,
                text: "Alpha block with mine words in it".into(),
            },
        )
        .unwrap();

    // The same block changed on both sides: a real conflict.
    let theirs = "- Alpha block with theirs words in it\n- Beta block stays as it is\n";
    std::fs::write(graph.join("pages/p.md"), theirs).unwrap();
    wait_for("dirty notice", Duration::from_secs(10), || {
        events.try_iter().find_map(|e| match e {
            RuntimeEvent::ExternalChangeWhileDirty { .. } => Some(()),
            _ => None,
        })
    });
    let report = s.queue().flush(Source::Ui).unwrap();
    assert_eq!(report.conflicts, vec![key]);
    assert_eq!(
        std::fs::read_to_string(graph.join("pages/p.md")).unwrap(),
        theirs
    );
    let _ = s.shutdown(Duration::from_secs(10));
}

#[test]
fn queue_graph_writer_maps_busy_and_stale() {
    let tmp = tempfile::tempdir().unwrap();
    let graph = graph_with(tmp.path(), &[("pages/p.md", PAGE)]);
    let mut cfg = config(&graph, &tmp.path().join("data"));
    cfg.watch = None;
    let s = Session::open(cfg).unwrap();
    let writer = QueueGraphWriter::new(s.queue().clone()).with_timeout(Duration::from_millis(200));

    {
        let mut lock = writer.acquire().unwrap();
        // A second acquire cannot get through while the first lock lives.
        assert!(matches!(writer.acquire(), Err(WriterError::Busy)));
        // Wrong expected content: nothing is applied.
        let err = lock
            .apply(&[FileChange::Write {
                path: "pages/p.md".into(),
                content: b"- x\n".to_vec(),
                expected: Some(b"- not what is on disk\n".to_vec()),
            }])
            .unwrap_err();
        assert!(matches!(err, WriterError::Stale(p) if p == "pages/p.md"));
        assert_eq!(
            std::fs::read_to_string(graph.join("pages/p.md")).unwrap(),
            PAGE
        );
        lock.apply(&[FileChange::Write {
            path: "pages/new.md".into(),
            content: b"- created by sync\n".to_vec(),
            expected: None,
        }])
        .unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(graph.join("pages/new.md")).unwrap(),
        "- created by sync\n"
    );
    wait_for("sync write indexed", Duration::from_secs(10), || {
        index_has(&s, "created by sync").then_some(())
    });
    assert!(s.shutdown(Duration::from_secs(10)).is_clean());
}

#[test]
fn mcp_endpoint_serves_with_the_index_reader() {
    let tmp = tempfile::tempdir().unwrap();
    let graph = graph_with(tmp.path(), &[("pages/p.md", PAGE)]);
    let mut cfg = config(&graph, &tmp.path().join("data"));
    cfg.mcp = Some(McpOptions {
        config: bitacora_mcp::McpConfig {
            port: 0,
            ..bitacora_mcp::McpConfig::default()
        },
        token_path: tmp.path().join("tokens.json"),
        secrets: None,
    });
    let s = Session::open(cfg).unwrap();
    let endpoint = s.mcp_endpoint().unwrap();
    let addr = endpoint
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap()
        .to_owned();
    let mut conn = std::net::TcpStream::connect(&addr).unwrap();
    conn.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(
        conn,
        "GET /health HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut out = String::new();
    conn.read_to_string(&mut out).unwrap();
    assert!(out.starts_with("HTTP/1.1 200"), "{out}");
    // No token -> rejected.
    let mut conn = std::net::TcpStream::connect(&addr).unwrap();
    conn.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(
        conn,
        "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut out = String::new();
    conn.read_to_string(&mut out).unwrap();
    assert!(out.starts_with("HTTP/1.1 401"), "{out}");
    assert!(s.shutdown(Duration::from_secs(10)).is_clean());
}

#[test]
fn shutdown_flushes_pending_edits_and_indexes_them() {
    let tmp = tempfile::tempdir().unwrap();
    let graph = graph_with(tmp.path(), &[("pages/p.md", PAGE)]);
    let data = tmp.path().join("data");
    let mut cfg = config(&graph, &data);
    cfg.debounce = None;
    let s = Session::open(cfg).unwrap();
    let key = s.open_page("pages/p.md").unwrap();
    let id = blocks(&s, &key)[2].0;
    s.queue()
        .execute(
            Source::Ui,
            Request::Run {
                label: "edit",
                cmd: Cmd::SetText {
                    id,
                    text: "Gamma at shutdown".into(),
                },
            },
        )
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(graph.join("pages/p.md")).unwrap(),
        PAGE
    );
    let report = s.shutdown(Duration::from_secs(10));
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(report.flush.unwrap().written.len(), 1);
    assert_eq!(
        std::fs::read_to_string(graph.join("pages/p.md")).unwrap(),
        "- Alpha\n- Beta\n- Gamma at shutdown\n"
    );

    // Reopening reconciles: the final flush was indexed before the writer stopped.
    let mut cfg = config(&graph, &data);
    cfg.watch = None;
    let s = Session::open(cfg).unwrap();
    assert_eq!(s.open_stats().unwrap().parsed, 0, "{:?}", s.open_stats());
    assert!(index_has(&s, "Gamma at shutdown"));
}
