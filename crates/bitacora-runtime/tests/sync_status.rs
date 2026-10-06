//! Runtime-level sync status stream, conflict resolution through the session, startup recovery
//! and per-page history with undoable restore (BIT-US-0047, BIT-US-0048).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use bitacora_core::editor::Cmd;
use bitacora_core::queue::Source;
use bitacora_runtime::{Session, SyncOptions};
use bitacora_sync::detect_git;
use bitacora_sync::history::DiffKind;
use bitacora_sync::merge::Resolution;
use bitacora_sync::onboarding::{OnboardingConfig, clone_graph, enable_sync};
use bitacora_sync::repo_setup::Identity;
use bitacora_sync::state::SyncState;
use bitacora_testkit::git::{git, init_bare};
use common::{blocks, config, graph_with, wait_for};

const PAGE: &str = "- first block is a long line\n- second block is another long line\n- third\n";

fn onboarding(name: &str) -> OnboardingConfig {
    let mut c = OnboardingConfig::new(detect_git(None));
    c.identity = Some(Identity {
        name: name.to_owned(),
        email: format!("{name}@example.com"),
    });
    c
}

fn open(graph: &Path, data: &Path, device: &str) -> Session {
    let mut cfg = config(graph, data);
    let mut opts = SyncOptions::new(device, "main");
    opts.tune = Some(std::sync::Arc::new(|ec| {
        ec.commit.idle = Duration::from_millis(300);
        ec.commit.max = Duration::from_secs(2);
    }));
    cfg.sync = Some(opts);
    Session::open(cfg).unwrap()
}

fn two_devices(tmp: &Path) -> (PathBuf, PathBuf) {
    let remote = tmp.join("remote.git");
    init_bare(&remote);
    let url = remote.to_string_lossy().into_owned();
    let a_dir = graph_with(&tmp.join("a"), &[("pages/p.md", PAGE)]);
    enable_sync(&a_dir, &url, "main", &onboarding("alice")).unwrap();
    let b_dir = tmp.join("b").join("graph");
    std::fs::create_dir_all(b_dir.parent().unwrap()).unwrap();
    clone_graph(&url, &b_dir, &onboarding("bob")).unwrap();
    (a_dir, b_dir)
}

fn push_conflicting_edit(b_dir: &Path) {
    std::fs::write(
        b_dir.join("pages/p.md"),
        PAGE.replace("second block", "second block (bob)"),
    )
    .unwrap();
    git(b_dir, &["add", "-A"]);
    git(b_dir, &["commit", "-m", "bob edit"]);
    git(b_dir, &["push", "origin", "HEAD:main"]);
}

#[test]
fn status_stream_conflict_resolution_and_restart_recovery() {
    let tmp = tempfile::tempdir().unwrap();
    let (a_dir, b_dir) = two_devices(tmp.path());
    let data = tmp.path().join("a-data");
    let a = open(&a_dir, &data, "alice");
    let watch = a.sync_watch().unwrap();
    let (_, first) = watch.current();
    assert_eq!(first.status.state, SyncState::Idle);
    assert_eq!(first.message, "Synced");
    assert!(first.backend.description.contains("git"));
    assert!(a.recovery_report().is_some());

    // A and B edit the same block differently; B pushes first.
    std::fs::write(
        a_dir.join("pages/p.md"),
        PAGE.replace("second block", "second block (alice)"),
    )
    .unwrap();
    push_conflicting_edit(&b_dir);
    assert!(a.sync_now());
    let view = wait_for("conflicted status", Duration::from_secs(30), || {
        let v = a.sync_view()?;
        (v.status.state == SyncState::Conflicted).then_some(v)
    });
    assert_eq!(view.message, "Conflicts (1)");
    assert_eq!(view.status.conflict_pages, ["pages/p.md"]);
    assert!(!view.can_retry);

    // The stream moved on: waiting on an old version returns the new view at once.
    let (version, _) = watch.current();
    assert!(version > 0);
    let (v2, _) = watch.wait_changed(0, Duration::from_millis(10));
    assert_eq!(v2, version);

    // Restart: shut the session down and reopen it; the conflict is restored from disk.
    let _ = a.shutdown(Duration::from_secs(10));
    let a = open(&a_dir, &data, "alice");
    let report = a.recovery_report().unwrap().clone();
    assert_eq!(report.restored_conflicts, 1, "{report:?}");
    let view = a.sync_view().unwrap();
    assert_eq!(view.status.state, SyncState::Conflicted);
    assert_eq!(view.status.conflict_pages, ["pages/p.md"]);

    // Resolve through the session (the engine thread applies it through the command queue).
    let outcome = a
        .resolve_conflict_page("pages/p.md", Resolution::Theirs)
        .unwrap();
    assert_eq!(outcome.remaining, 0);
    let view = wait_for("idle after resolve", Duration::from_secs(30), || {
        let v = a.sync_view()?;
        (v.status.state == SyncState::Idle && v.status.conflicts == 0).then_some(v)
    });
    assert!(view.status.conflict_pages.is_empty());
    assert!(
        std::fs::read_to_string(a_dir.join("pages/p.md"))
            .unwrap()
            .contains("(bob)")
    );
    assert_eq!(
        git(&a_dir, &["rev-parse", "HEAD"]),
        git(&a_dir, &["rev-parse", "origin/main"]),
        "the resolved merge was pushed"
    );
    // No engine: the resolve call reports it instead of hanging.
    let _ = a.shutdown(Duration::from_secs(10));
}

#[test]
fn history_diff_and_undoable_restore() {
    let tmp = tempfile::tempdir().unwrap();
    let (a_dir, _b_dir) = two_devices(tmp.path());
    let a = open(&a_dir, &tmp.path().join("a-data"), "alice");

    // Edit block 2 and delete block 3 through core, flush and commit.
    let key = a.open_page("pages/p.md").unwrap();
    let ids = blocks(&a, &key);
    a.queue()
        .run(
            Source::Ui,
            "edit",
            Cmd::SetText {
                id: ids[1].0,
                text: "second block is another long line, edited".into(),
            },
        )
        .unwrap();
    a.queue()
        .run(
            Source::Ui,
            "delete",
            Cmd::DeleteBlocks {
                ids: vec![ids[2].0],
            },
        )
        .unwrap();
    a.queue().flush(Source::Ui).unwrap();
    a.sync_now();
    wait_for("sync after edit", Duration::from_secs(30), || {
        let v = a.sync_view()?;
        (v.status.state == SyncState::Idle
            && v.status.ahead == 0
            && git(&a_dir, &["log", "--oneline"]).lines().count() >= 2)
            .then_some(())
    });

    let history = a.page_history("pages/p.md", 20).unwrap();
    assert_eq!(history.len(), 2, "{history:#?}");
    let original = &history[1];
    assert_eq!(a.history_version(original).unwrap(), PAGE);

    let diff = a.history_diff("pages/p.md", original).unwrap();
    let kinds: Vec<DiffKind> = diff.visible(false).map(|b| b.kind).collect();
    assert!(kinds.contains(&DiffKind::Changed), "{diff:?}");
    assert!(kinds.contains(&DiffKind::Removed), "{diff:?}");

    // Restore just the deleted block.
    let removed: Vec<_> = diff
        .blocks
        .iter()
        .filter(|b| b.kind == DiffKind::Removed)
        .cloned()
        .collect();
    let report = a.restore_blocks("pages/p.md", original, &removed).unwrap();
    assert_eq!(report.restored, 1, "{report:?}");
    a.queue().flush(Source::Ui).unwrap();
    let text = std::fs::read_to_string(a_dir.join("pages/p.md")).unwrap();
    assert_eq!(
        text,
        "- first block is a long line\n- second block is another long line, edited\n- third\n"
    );

    // Undo brings the deletion back.
    a.undo_restore(&report).unwrap();
    a.queue().flush(Source::Ui).unwrap();
    assert_eq!(
        std::fs::read_to_string(a_dir.join("pages/p.md")).unwrap(),
        "- first block is a long line\n- second block is another long line, edited\n"
    );

    // Restore the whole page, then undo that too.
    let report = a.restore_page("pages/p.md", original).unwrap();
    a.queue().flush(Source::Ui).unwrap();
    assert_eq!(
        std::fs::read_to_string(a_dir.join("pages/p.md")).unwrap(),
        PAGE
    );
    a.undo_restore(&report).unwrap();
    a.queue().flush(Source::Ui).unwrap();
    assert_eq!(
        std::fs::read_to_string(a_dir.join("pages/p.md")).unwrap(),
        "- first block is a long line\n- second block is another long line, edited\n"
    );
    let _ = a.shutdown(Duration::from_secs(10));
}
