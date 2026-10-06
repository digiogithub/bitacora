//! Auto-commit integration tests (BIT-US-0044): messages, squashing, ignored files, lock.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use bitacora_sync::autocommit::{
    CommitError, CommitOutcome, CommitRequest, SkipReason, commit_changes, commit_locked,
};
use bitacora_sync::backend::CommitKind;
use bitacora_sync::commit_msg::parse_message;
use bitacora_sync::engine::{Command, EngineConfig, SyncEngine, SystemTiming, Timing, spawn};
use bitacora_sync::state::{MemoryMergeStore, SyncState};
use bitacora_sync::writer::WriterError;
use common::{Dev, KINDS, World};

fn commit_count(d: &Dev) -> usize {
    d.git(&["rev-list", "--count", "HEAD"]).parse().unwrap()
}

fn auto(d: &mut Dev) -> CommitOutcome {
    d.engine.commit_now(&CommitRequest::auto()).unwrap()
}

fn world(kind: common::Kind) -> (World, Dev) {
    let w = World::new(kind);
    let a = w.first_device(&[("pages/Home.md", "- home\n")]);
    (w, a)
}

#[test]
fn auto_commit_message_has_trailers() {
    for kind in KINDS {
        let (_w, mut a) = world(kind);
        a.write("journals/2026_10_06.md", "- today\n");
        a.write("pages/Project X.md", "- x\n");
        a.write("pages/Ideas.md", "- i\n");
        let CommitOutcome::Committed { amended, pages, .. } = auto(&mut a) else {
            panic!("{kind:?}: expected a commit");
        };
        assert!(!amended, "{kind:?}: HEAD is the pushed migrate commit");
        assert_eq!(pages.len(), 3);
        let msg = a.git(&["log", "-1", "--format=%B"]);
        assert_eq!(
            msg.lines().next().unwrap(),
            "bitacora: edit 3 pages (journals/2026_10_06, Ideas, Project X)",
            "{kind:?}"
        );
        let parsed = parse_message(&msg);
        assert_eq!(parsed.kind, Some(CommitKind::Auto));
        assert_eq!(parsed.device.as_deref(), Some("alice"));
        assert_eq!(
            parsed.pages,
            [
                "journals/2026_10_06.md",
                "pages/Ideas.md",
                "pages/Project X.md"
            ]
        );
        assert_eq!(*a.engine.state(), SyncState::Idle, "{kind:?}");
    }
}

#[test]
fn unpushed_auto_commits_are_squashed_with_page_union() {
    for kind in KINDS {
        let (_w, mut a) = world(kind);
        a.write("pages/One.md", "- 1\n");
        assert!(matches!(
            auto(&mut a),
            CommitOutcome::Committed { amended: false, .. }
        ));
        let parent = a.git(&["rev-parse", "HEAD~1"]);
        let count = commit_count(&a);

        a.timing.advance(Duration::from_secs(10 * 60));
        a.write("pages/Two.md", "- 2\n");
        let out = auto(&mut a);
        assert!(
            matches!(out, CommitOutcome::Committed { amended: true, .. }),
            "{kind:?}: {out:?}"
        );
        assert_eq!(commit_count(&a), count, "{kind:?}: amended, not added");
        assert_eq!(
            a.git(&["rev-parse", "HEAD~1"]),
            parent,
            "{kind:?}: same parent"
        );
        let parsed = parse_message(&a.git(&["log", "-1", "--format=%B"]));
        assert_eq!(parsed.pages, ["pages/One.md", "pages/Two.md"], "{kind:?}");
        assert_eq!(
            a.git(&["ls-tree", "-r", "--name-only", "HEAD"])
                .matches("pages/")
                .count(),
            3
        );
    }
}

#[test]
fn squash_window_device_and_kind_limits() {
    for kind in KINDS {
        let (_w, mut a) = world(kind);
        a.write("pages/One.md", "- 1\n");
        auto(&mut a);
        let count = commit_count(&a);
        let backend = bitacora_sync::backend::select_backend(
            &common::onboarding_config(kind, "alice").detection,
            &a.dir,
            Default::default(),
        )
        .unwrap();
        let settings = bitacora_sync::autocommit::CommitSettings::new("alice", "origin", "main");

        // Older than the 30 minute window: new commit.
        a.write("pages/Two.md", "- 2\n");
        let later = a.timing.unix_now() + 31 * 60;
        let out =
            commit_changes(backend.as_ref(), &settings, &CommitRequest::auto(), later).unwrap();
        assert!(
            matches!(out, CommitOutcome::Committed { amended: false, .. }),
            "{kind:?}"
        );
        assert_eq!(commit_count(&a), count + 1);

        // Another device never squashes into ours.
        a.write("pages/Three.md", "- 3\n");
        let other = bitacora_sync::autocommit::CommitSettings::new("laptop", "origin", "main");
        let out = commit_changes(
            backend.as_ref(),
            &other,
            &CommitRequest::auto(),
            a.timing.unix_now(),
        )
        .unwrap();
        assert!(
            matches!(out, CommitOutcome::Committed { amended: false, .. }),
            "{kind:?}"
        );

        // Agent commits are never squashed and carry the agent trailer.
        a.write("pages/Four.md", "- 4\n");
        let req = CommitRequest {
            kind: CommitKind::Agent,
            agent: Some("claude-desktop".into()),
            subject: None,
        };
        let out = commit_changes(backend.as_ref(), &settings, &req, a.timing.unix_now()).unwrap();
        assert!(
            matches!(out, CommitOutcome::Committed { amended: false, .. }),
            "{kind:?}"
        );
        let msg = a.git(&["log", "-1", "--format=%B"]);
        assert!(
            msg.contains("Bitacora-Kind: agent") && msg.contains("Bitacora-Agent: claude-desktop"),
            "{msg}"
        );

        // Squash disabled: a fresh auto commit after an auto commit is a new commit.
        a.write("pages/Five.md", "- 5\n");
        auto(&mut a);
        let n = commit_count(&a);
        let mut off = settings.clone();
        off.squash = false;
        a.write("pages/Six.md", "- 6\n");
        let out = commit_changes(
            backend.as_ref(),
            &off,
            &CommitRequest::auto(),
            a.timing.unix_now(),
        )
        .unwrap();
        assert!(
            matches!(out, CommitOutcome::Committed { amended: false, .. }),
            "{kind:?}"
        );
        assert_eq!(commit_count(&a), n + 1);
    }
}

#[test]
fn pushed_commits_are_never_amended() {
    for kind in KINDS {
        let (w, mut a) = world(kind);
        a.write("pages/One.md", "- 1\n");
        auto(&mut a);
        assert_eq!(a.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(a.head(), w.remote_head(), "{kind:?}: pushed");
        let pushed = a.head();

        a.write("pages/Two.md", "- 2\n");
        let out = auto(&mut a);
        assert!(
            matches!(out, CommitOutcome::Committed { amended: false, .. }),
            "{kind:?}"
        );
        assert_eq!(
            a.git(&["rev-parse", "HEAD~1"]),
            pushed,
            "{kind:?}: parent is the pushed commit"
        );
        assert_eq!(
            w.remote_head(),
            pushed,
            "{kind:?}: remote untouched by the local commit"
        );
    }
}

#[test]
fn only_ignored_changes_skip_the_commit() {
    for kind in KINDS {
        let (_w, mut a) = world(kind);
        let head = a.head();
        a.write("logseq/bak/pages/Ideas/2026-10-06.md", "- backup\n");
        a.write(".DS_Store", "x");
        let out = auto(&mut a);
        assert!(
            matches!(out, CommitOutcome::Skipped(_)),
            "{kind:?}: {out:?}"
        );
        assert_eq!(a.head(), head);
        assert_eq!(*a.engine.state(), SyncState::Idle);
    }
}

#[test]
fn clean_tree_is_skipped() {
    let (_w, mut a) = world(common::Kind::Hybrid);
    assert_eq!(auto(&mut a), CommitOutcome::Skipped(SkipReason::Clean));
}

#[test]
fn commit_runs_under_the_graph_lock_and_waits_for_pending_writes() {
    let (_w, mut a) = world(common::Kind::Hybrid);
    a.write("pages/One.md", "- 1\n");
    let before = a.writer.acquire_count();
    auto(&mut a);
    assert_eq!(
        a.writer.acquire_count(),
        before + 1,
        "staging took the lock"
    );

    // A pending write transaction: the commit step reports busy and commits nothing.
    a.write("pages/Two.md", "- 2\n");
    a.writer.set_busy(true);
    let head = a.head();
    let backend = bitacora_sync::backend::select_backend(
        &common::onboarding_config(common::Kind::Hybrid, "alice").detection,
        &a.dir,
        Default::default(),
    )
    .unwrap();
    let settings = bitacora_sync::autocommit::CommitSettings::new("alice", "origin", "main");
    let err = commit_locked(
        backend.as_ref(),
        a.writer.as_ref(),
        &settings,
        &CommitRequest::auto(),
        0,
    )
    .unwrap_err();
    assert!(matches!(err, CommitError::Writer(WriterError::Busy)));
    assert_eq!(a.head(), head);
    a.writer.set_busy(false);
}

#[test]
fn idle_debounce_commits_on_the_engine_thread() {
    let w = World::new(common::Kind::Hybrid);
    let a = w.first_device(&[("pages/Home.md", "- home\n")]);
    let dir = a.dir.clone();
    drop(a);
    let backend = bitacora_sync::backend::select_backend(
        &common::onboarding_config(common::Kind::Hybrid, "alice").detection,
        &dir,
        Default::default(),
    )
    .unwrap();
    let writer = Arc::new(bitacora_sync::writer::testing::DirGraphWriter::new(&dir));
    let mut cfg = EngineConfig::new(&dir, "alice", "main");
    cfg.commit.idle = Duration::from_millis(300);
    cfg.commit.max = Duration::from_secs(5);
    cfg.fetch_foreground = Duration::from_secs(3600);
    let engine = SyncEngine::new(
        backend,
        writer,
        Box::new(MemoryMergeStore::new()),
        cfg,
        Arc::new(SystemTiming),
    );
    let handle = spawn(engine);
    let initial = bitacora_testkit::git::git(&dir, &["rev-parse", "HEAD"]);

    std::fs::write(dir.join("pages/Ideas.md"), "- idea\n").unwrap();
    handle.send(Command::FileFlushed);
    // Not committed while inside the idle window.
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        bitacora_testkit::git::git(&dir, &["rev-parse", "HEAD"]),
        initial
    );

    // Committed (and pushed by the follow-up sync) after the debounce.
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let head = bitacora_testkit::git::git(&dir, &["rev-parse", "HEAD"]);
        let remote = bitacora_testkit::git::git(&w.remote, &["rev-parse", "main"]);
        if head != initial && head == remote && handle.status().state == SyncState::Idle {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no idle commit happened"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        bitacora_testkit::git::git(&dir, &["rev-list", "--count", "HEAD"]),
        "2"
    );
    assert_eq!(handle.status().state, SyncState::Idle);
    let engine = handle.shutdown().unwrap();
    assert!(engine.status().last_sync.is_some());
}
