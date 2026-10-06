//! Two runtimes syncing through a temp bare repository (BIT-US-0045, BIT-SP-0006).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;
use std::time::Duration;

use bitacora_core::editor::Cmd;
use bitacora_core::queue::{QueueEvent, Source};
use bitacora_runtime::{RuntimeEvent, Session, SyncOptions};
use bitacora_sync::GitDetection;
use bitacora_sync::detect_git;
use bitacora_sync::onboarding::{OnboardingConfig, clone_graph, enable_sync};
use bitacora_sync::repo_setup::Identity;
use bitacora_sync::state::SyncState;
use bitacora_testkit::git::{git, git_available, init_bare};
use common::{PAGE, blocks, config, graph_with, index_has, wait_for};

fn onboarding(name: &str) -> OnboardingConfig {
    let mut c = OnboardingConfig::new(detect_git(None));
    c.identity = Some(Identity {
        name: name.to_owned(),
        email: format!("{name}@example.com"),
    });
    c
}

fn open(graph: &Path, data: &Path, device: &str, background: bool) -> Session {
    let mut cfg = config(graph, data);
    let mut opts = SyncOptions::new(device, "main");
    opts.background = background;
    opts.detection = Some(match detect_git(None) {
        d @ GitDetection::Found { .. } => d,
        other => other,
    });
    opts.tune = Some(std::sync::Arc::new(|ec| {
        ec.commit.idle = Duration::from_millis(300);
        ec.commit.max = Duration::from_secs(2);
    }));
    cfg.sync = Some(opts);
    Session::open(cfg).unwrap()
}

#[test]
fn two_runtimes_merge_a_metadata_change_and_a_text_change() {
    assert!(git_available());
    let tmp = tempfile::tempdir().unwrap();
    let remote = tmp.path().join("remote.git");
    init_bare(&remote);
    let url = remote.to_string_lossy().into_owned();

    // Device A starts the repository, device B clones it.
    let a_dir = graph_with(&tmp.path().join("a"), &[("pages/p.md", PAGE)]);
    enable_sync(&a_dir, &url, "main", &onboarding("alice")).unwrap();
    let b_dir = tmp.path().join("b").join("graph");
    std::fs::create_dir_all(b_dir.parent().unwrap()).unwrap();
    clone_graph(&url, &b_dir, &onboarding("bob")).unwrap();

    let a = open(&a_dir, &tmp.path().join("a-data"), "alice", false);
    let b = open(&b_dir, &tmp.path().join("b-data"), "bob", false);
    let b_events = b.subscribe();

    // A collapses "Beta" (metadata only); B edits "Gamma" (content).
    let a_key = a.open_page("pages/p.md").unwrap();
    let beta = blocks(&a, &a_key)[1].0;
    a.queue()
        .run(
            Source::Ui,
            "collapse",
            Cmd::SetCollapsed {
                ids: vec![beta],
                collapsed: true,
            },
        )
        .unwrap();
    let b_key = b.open_page("pages/p.md").unwrap();
    let gamma = blocks(&b, &b_key)[2].0;
    b.queue()
        .run(
            Source::Ui,
            "edit",
            Cmd::SetText {
                id: gamma,
                text: "Gamma by bob".into(),
            },
        )
        .unwrap();

    let (state, _) = a.sync_once().unwrap();
    assert_eq!(state, SyncState::Idle);
    let (state, status) = b.sync_once().unwrap();
    assert_eq!(state, SyncState::Idle, "{status:?}");
    let (state, _) = a.sync_once().unwrap();
    assert_eq!(state, SyncState::Idle);

    let want = "- Alpha\n- Beta\n  collapsed:: true\n- Gamma by bob\n";
    for dir in [&a_dir, &b_dir] {
        let text = std::fs::read_to_string(dir.join("pages/p.md")).unwrap();
        assert_eq!(text, want);
        assert!(!text.contains("<<<<<<<"));
    }
    assert_eq!(
        git(&a_dir, &["rev-parse", "HEAD"]),
        git(&remote, &["rev-parse", "main"])
    );
    assert_eq!(git(&a_dir, &["status", "--porcelain"]), "");
    assert_eq!(git(&b_dir, &["status", "--porcelain"]), "");

    // The merge went through B's queue: its loaded page was reloaded and the index follows.
    wait_for("index of the merge", Duration::from_secs(10), || {
        index_has(&b, "Gamma by bob").then_some(())
    });
    assert!(b_events.try_iter().any(|e| matches!(
        e,
        RuntimeEvent::Queue(QueueEvent::FilesApplied { .. })
    ) || matches!(
        e,
        RuntimeEvent::Queue(QueueEvent::Flushed(_))
    )),);
    assert!(a.shutdown(Duration::from_secs(20)).is_clean());
    assert!(b.shutdown(Duration::from_secs(20)).is_clean());
}

#[test]
fn background_engine_auto_commits_after_a_flush() {
    assert!(git_available());
    let tmp = tempfile::tempdir().unwrap();
    let remote = tmp.path().join("remote.git");
    init_bare(&remote);
    let url = remote.to_string_lossy().into_owned();
    let dir = graph_with(&tmp.path().join("a"), &[("pages/p.md", PAGE)]);
    enable_sync(&dir, &url, "main", &onboarding("alice")).unwrap();
    let before = git(&dir, &["rev-list", "--count", "HEAD"]);

    let s = open(&dir, &tmp.path().join("data"), "alice", true);
    assert!(s.sync_status().is_some());
    let key = s.open_page("pages/p.md").unwrap();
    let id = blocks(&s, &key)[0].0;
    s.queue()
        .run(
            Source::Ui,
            "edit",
            Cmd::SetText {
                id,
                text: "Alpha auto".into(),
            },
        )
        .unwrap();
    s.queue().flush(Source::Ui).unwrap();

    wait_for("auto-commit", Duration::from_secs(20), || {
        (git(&dir, &["rev-list", "--count", "HEAD"]) != before).then_some(())
    });
    assert!(git(&dir, &["show", "HEAD:pages/p.md"]).contains("Alpha auto"));
    assert!(s.shutdown(Duration::from_secs(20)).is_clean());
}
