//! Command queue tests (BIT-US-0062; BIT-SP-0005.R1).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;

use bitacora_core::editor::{Cmd, MemStore, Workspace};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;
use bitacora_core::queue::{
    CommandQueue, FileEdit, QueueConfig, QueueError, QueueEvent, QueueJoin, Request, Response,
    Source,
};

fn key() -> PageKey {
    PageKey::from_title("p")
}

fn path() -> GraphPath {
    GraphPath::new("pages/p.md").expect("path")
}

fn spawn_with(src: &str, cfg: QueueConfig) -> (CommandQueue, QueueJoin) {
    let mut ws = Workspace::new();
    ws.load_page(key(), "p", Some(path()), src.as_bytes());
    let mut store = MemStore::default();
    store.files.insert(path(), src.as_bytes().to_vec());
    CommandQueue::spawn(ws, Box::new(store), cfg)
}

fn push(q: &CommandQueue, source: Source, text: &str) {
    q.run(
        source,
        "Insert",
        Cmd::InsertChild {
            page: key(),
            parent: None,
            text: text.to_owned(),
        },
    )
    .expect("insert");
}

#[test]
fn two_producers_thousand_commands_each_all_applied_in_a_consistent_order() {
    let (q, join) = spawn_with("- seed\n", QueueConfig::default());
    let handles: Vec<_> = (0..2)
        .map(|p| {
            let q = q.clone();
            std::thread::spawn(move || {
                let src = if p == 0 { Source::Ui } else { Source::Mcp };
                for i in 0..1000 {
                    push(&q, src, &format!("p{p}-{i:04}"));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().expect("producer");
    }
    let ws = join.shutdown().expect("shutdown");
    let page = ws.page(&key()).expect("page");
    let texts: Vec<String> = page
        .roots
        .iter()
        .map(|id| page.block(*id).expect("b").text.clone())
        .collect();
    assert_eq!(texts.len(), 2001);
    assert_eq!(texts[0], "seed");
    // Each producer's commands keep their own submission order.
    for p in 0..2 {
        let mine: Vec<&String> = texts
            .iter()
            .filter(|t| t.starts_with(&format!("p{p}-")))
            .collect();
        assert_eq!(mine.len(), 1000);
        assert!(
            mine.windows(2).all(|w| w[0] < w[1]),
            "producer {p} reordered"
        );
    }
    // The audit trail has one entry per command, in application order, tagged by source.
    let audit = q.audit();
    assert_eq!(audit.len(), 2000);
    assert!(audit.windows(2).all(|w| w[0].seq < w[1].seq));
    assert_eq!(
        audit.iter().filter(|a| a.source == Source::Mcp).count(),
        1000
    );
    assert_eq!(
        audit.iter().filter(|a| a.source == Source::Ui).count(),
        1000
    );
}

#[test]
fn identical_submission_order_gives_identical_state() {
    let run = || {
        let (q, join) = spawn_with("- a\n- b\n", QueueConfig::default());
        for i in 0..50 {
            push(&q, Source::Ui, &format!("n{i}"));
        }
        let ws = join.shutdown().expect("s");
        String::from_utf8(ws.page(&key()).expect("p").serialize()).expect("utf8")
    };
    assert_eq!(run(), run());
}

#[test]
fn refusals_and_errors_are_returned_and_state_is_unchanged() {
    let (q, join) = spawn_with("- a\n", QueueConfig::default());
    let s0 = q.snapshot(&key()).expect("snap");
    let a = s0.blocks[0].id;
    let err = q
        .run(Source::Mcp, "Indent", Cmd::Indent { ids: vec![a] })
        .expect_err("refused");
    assert!(matches!(err, QueueError::Commit(_)), "{err:?}");
    assert_eq!(q.snapshot(&key()).expect("snap").version, s0.version);
    assert!(q.audit().is_empty());
    drop(join);
}

#[test]
fn snapshots_never_block_and_reflect_commits_and_flush() {
    let (q, join) = spawn_with("- a\n\t- a1\n", QueueConfig::default());
    let s = q.snapshot(&key()).expect("snap");
    assert_eq!(
        s.blocks.iter().map(|b| b.depth).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(!s.dirty);
    let a = s.blocks[0].id;
    q.run(
        Source::Ui,
        "Set",
        Cmd::SetText {
            id: a,
            text: "A".into(),
        },
    )
    .expect("run");
    let s2 = q.snapshot(&key()).expect("snap");
    assert_eq!(s2.blocks[0].text, "A");
    assert!(s2.dirty && s2.version > s.version);
    assert_eq!(s.blocks[0].text, "a", "old snapshot is immutable");
    let report = q.flush(Source::Ui).expect("flush");
    assert!(report.is_complete());
    assert!(!q.snapshot(&key()).expect("snap").dirty);
    drop(join);
}

#[test]
fn observers_see_commits_and_flushed_files() {
    let events: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = events.clone();
    let cfg = QueueConfig {
        observers: vec![Box::new(move |e| {
            let s = match e {
                QueueEvent::Committed { source, .. } => format!("commit:{source:?}"),
                QueueEvent::Flushed(r) => format!("flush:{}", r.written.len()),
                QueueEvent::FilesApplied { written, .. } => format!("applied:{}", written.len()),
                QueueEvent::PageReloaded(_) => "reloaded".to_owned(),
            };
            sink.lock().expect("lock").push(s);
        })],
        auto_flush: true,
        ..QueueConfig::default()
    };
    let (q, join) = spawn_with("- a\n", cfg);
    push(&q, Source::Mcp, "x");
    drop(join.shutdown());
    assert_eq!(*events.lock().expect("lock"), vec!["commit:Mcp", "flush:1"]);
}

#[test]
fn load_page_is_refused_while_dirty() {
    let (q, join) = spawn_with("- a\n", QueueConfig::default());
    push(&q, Source::Ui, "x");
    let load = |q: &CommandQueue| {
        q.execute(
            Source::External,
            Request::LoadPage {
                key: key(),
                title: "p".into(),
                path: Some(path()),
                bytes: b"- other\n".to_vec(),
            },
        )
    };
    let err = load(&q).expect_err("dirty");
    assert!(matches!(err, QueueError::PageDirty(_)));
    q.flush(Source::Ui).expect("flush");
    assert!(matches!(load(&q), Ok(Response::Loaded)));
    assert_eq!(q.snapshot(&key()).expect("s").blocks[0].text, "other");
    drop(join);
}

#[test]
fn lock_flushes_blocks_other_writers_and_applies_with_expected_check() {
    let (q, join) = spawn_with("- a\n", QueueConfig::default());
    push(&q, Source::Ui, "dirty");
    let mut lock = q.acquire(Duration::from_secs(2)).expect("acquire");
    // acquire flushed the pending edit
    assert!(!q.snapshot(&key()).expect("s").dirty);

    // other writers wait while the lock is held
    let q2 = q.clone();
    let blocked = std::thread::spawn(move || push(&q2, Source::Ui, "after-lock"));
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        q.snapshot(&key())
            .expect("s")
            .blocks
            .iter()
            .all(|b| b.text != "after-lock")
    );

    // stale expectation: nothing is applied
    let err = lock
        .apply(vec![
            FileEdit::Write {
                path: "pages/other.md".into(),
                content: b"- o\n".to_vec(),
                expected: None,
            },
            FileEdit::Write {
                path: "pages/p.md".into(),
                content: b"- x\n".to_vec(),
                expected: Some(b"- WRONG\n".to_vec()),
            },
        ])
        .expect_err("stale");
    assert!(
        matches!(err, QueueError::Stale(ref p) if p == "pages/p.md"),
        "{err:?}"
    );

    // correct expectation: the loaded page is replaced by the new content
    let cur = b"- a\n- dirty\n".to_vec();
    lock.apply(vec![FileEdit::Write {
        path: "pages/p.md".into(),
        content: b"- merged\n".to_vec(),
        expected: Some(cur),
    }])
    .expect("apply");
    assert_eq!(q.snapshot(&key()).expect("s").blocks[0].text, "merged");
    drop(lock);
    blocked
        .join()
        .expect("blocked producer finishes after release");
    assert!(
        q.snapshot(&key())
            .expect("s")
            .blocks
            .iter()
            .any(|b| b.text == "after-lock")
    );
    assert!(q.audit().iter().any(|a| a.source == Source::Sync));
    drop(join);
}

#[test]
fn acquire_times_out_with_busy_and_does_not_leave_a_lock_behind() {
    let (q, join) = spawn_with("- a\n", QueueConfig::default());
    let lock = q.acquire(Duration::from_secs(2)).expect("first");
    let err = q.acquire(Duration::from_millis(50)).expect_err("busy");
    assert!(matches!(err, QueueError::Busy));
    drop(lock);
    // the cancelled acquire must not keep the writer parked
    push(&q, Source::Ui, "still alive");
    drop(join);
}

#[test]
fn reply_works_with_async_executors() {
    struct ThreadWaker(std::thread::Thread);
    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let (q, join) = spawn_with("- a\n", QueueConfig::default());
    let mut fut = Box::pin(q.submit(
        Source::Mcp,
        Request::Run {
            label: "Insert",
            cmd: Cmd::InsertChild {
                page: key(),
                parent: None,
                text: "async".into(),
            },
        },
    ));
    let waker = Waker::from(Arc::new(ThreadWaker(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let out = loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => break v,
            Poll::Pending => std::thread::park_timeout(Duration::from_secs(2)),
        }
    };
    assert!(matches!(out, Some(Ok(Response::Committed(_)))));
    drop(join);
}

#[test]
fn closed_queue_reports_closed() {
    let (q, join) = spawn_with("- a\n", QueueConfig::default());
    drop(join.shutdown());
    let err = q.run(
        Source::Ui,
        "x",
        Cmd::InsertChild {
            page: key(),
            parent: None,
            text: "y".into(),
        },
    );
    assert!(matches!(err, Err(QueueError::Closed)), "{err:?}");
}
