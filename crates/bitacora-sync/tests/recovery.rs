//! Startup recovery and status reporting (BIT-US-0047, BIT-SP-0006.R5/R6/R8/R15), against both
//! backends with real temp repositories.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;
use std::time::{Duration, SystemTime};

use bitacora_sync::backend::{ActiveBackend, CliConfig, select_backend};
use bitacora_sync::engine::{EngineConfig, SyncEngine};
use bitacora_sync::merge::Resolution;
use bitacora_sync::recovery::ExternalOperation;
use bitacora_sync::state::{INSTALL_GIT_HINT, SyncError, SyncState};
use bitacora_sync::store::JsonMergeStore;
use bitacora_sync::writer::testing::DirGraphWriter;
use common::{Dev, KINDS, Kind, World, assert_history_legal, onboarding_config};
use std::sync::Arc;

const P: &str =
    "- one is a fairly long first block\n- the contested block is here\n- three closes\n";

/// An engine persisting its merge state in `.git/bitacora/merge-state.json`, like the runtime.
fn json_engine(
    kind: Kind,
    dir: &Path,
    device: &str,
    timing: Arc<common::ManualTiming>,
) -> SyncEngine {
    let cfg = onboarding_config(kind, device);
    let backend = select_backend(&cfg.detection, dir, CliConfig::default()).unwrap();
    SyncEngine::new(
        backend,
        Arc::new(DirGraphWriter::new(dir)),
        Box::new(JsonMergeStore::for_graph(dir).unwrap()),
        EngineConfig::new(dir, device, "main"),
        timing,
    )
}

fn world(kind: Kind) -> (World, Dev) {
    let w = World::new(kind);
    let mut a = w.first_device(&[("pages/P.md", P)]);
    assert_eq!(a.engine.sync_now(), SyncState::Idle);
    (w, a)
}

#[test]
fn stale_index_lock_is_removed_and_a_fresh_one_kept() {
    for kind in KINDS {
        let (_w, mut a) = world(kind);
        let lock = a.dir.join(".git/index.lock");
        std::fs::write(&lock, "").unwrap();
        let report = a.engine.recover();
        assert!(!report.stale_lock_removed, "{kind:?}: a fresh lock stays");
        assert!(lock.exists());

        let f = std::fs::File::options().write(true).open(&lock).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(3600))
            .unwrap();
        drop(f);
        // Other tests of this binary run external processes concurrently, which the probe sees
        // as a live process: retry until none is running.
        let mut removed = false;
        for _ in 0..100 {
            removed = a.engine.recover().stale_lock_removed;
            if removed {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(removed, "{kind:?}");
        assert!(!lock.exists());
        // The repository works again.
        a.write("pages/P.md", &format!("{P}- more\n"));
        assert_eq!(a.engine.sync_now(), SyncState::Idle, "{kind:?}");
    }
}

#[test]
fn external_merge_is_reported_and_can_be_aborted() {
    for kind in KINDS {
        let (_w, mut a) = world(kind);
        let head = a.head();
        std::fs::write(a.dir.join(".git/MERGE_HEAD"), format!("{head}\n")).unwrap();
        std::fs::write(a.dir.join(".git/MERGE_MSG"), "Merge x\n").unwrap();
        let report = a.engine.recover();
        assert_eq!(report.external_operation, Some(ExternalOperation::Merge));
        assert_eq!(
            *a.engine.state(),
            SyncState::Error(SyncError::ExternalOperationInProgress),
            "{kind:?}"
        );
        let status = a.engine.status();
        assert!(
            status.user_message().contains("merge or rebase"),
            "{kind:?}"
        );
        assert!(status.can_retry());
        // A sync attempt keeps refusing.
        assert_eq!(
            a.engine.sync_now(),
            SyncState::Error(SyncError::ExternalOperationInProgress)
        );
        a.engine.abort_external_operation().unwrap();
        assert!(!a.dir.join(".git/MERGE_HEAD").exists());
        assert_eq!(*a.engine.state(), SyncState::Idle, "{kind:?}");
        assert_eq!(a.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_history_legal(&a.engine);
    }
}

#[test]
fn uncommitted_work_of_an_interrupted_session_is_scheduled_for_commit() {
    for kind in KINDS {
        let (_w, mut a) = world(kind);
        a.write("pages/New.md", "- written before the crash\n");
        let report = a.engine.recover();
        assert_eq!(report.uncommitted_paths, 1, "{kind:?}");
        assert_eq!(*a.engine.state(), SyncState::Dirty, "{kind:?}");
        assert!(a.engine.next_deadline().is_some());
        assert_eq!(a.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert!(
            a.git(&["log", "-1", "--name-only", "--format="])
                .contains("pages/New.md")
        );
    }
}

#[test]
fn conflict_state_survives_a_restart_and_resolves_through_the_new_engine() {
    for kind in KINDS {
        let w = World::new(kind);
        let mut a = w.first_device(&[("pages/P.md", P)]);
        assert_eq!(a.engine.sync_now(), SyncState::Idle);
        let mut b = w.clone_device();
        assert_eq!(b.engine.sync_now(), SyncState::Idle);
        a.engine = json_engine(kind, &a.dir, "alice", a.timing.clone());
        a.write("pages/P.md", &P.replace("contested", "contested, A"));
        b.write("pages/P.md", &P.replace("contested", "contested, B"));
        assert_eq!(b.engine.sync_now(), SyncState::Idle);
        assert_eq!(a.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        let status = a.engine.status();
        assert_eq!(status.conflicts, 1);
        assert_eq!(status.conflict_pages, ["pages/P.md"], "{kind:?}");
        assert_eq!(status.behind, 1, "{kind:?}: remote has B's commit");
        assert_eq!(status.user_message(), "Conflicts (1)");

        // "Restart": a brand new engine over the same repository.
        let mut restarted = json_engine(kind, &a.dir, "alice", a.timing.clone());
        assert_eq!(*restarted.state(), SyncState::Conflicted, "{kind:?}");
        let report = restarted.recover();
        assert_eq!(report.restored_conflicts, 1, "{kind:?}");
        assert_eq!(report.external_operation, None);
        assert_eq!(restarted.status().conflict_pages, ["pages/P.md"]);
        let id = restarted.pending_merge().unwrap().conflicts[0].id.clone();
        let outcome = restarted.resolve_conflict(&id, Resolution::Theirs).unwrap();
        assert_eq!(outcome.remaining, 0, "{kind:?}");
        assert_eq!(outcome.state, SyncState::Idle, "{kind:?}");
        assert!(a.read("pages/P.md").contains("contested, B"));
    }
}

#[test]
fn markers_written_by_another_tool_are_picked_up_at_startup() {
    for kind in KINDS {
        let (_w, mut a) = world(kind);
        a.write(
            "pages/P.md",
            "- one is a fairly long first block\n<<<<<<< HEAD\n- contested A\n=======\n- contested B\n>>>>>>> other\n- three closes\n",
        );
        let report = a.engine.recover();
        // Either the markers were split and repaired (file clean) or registered as conflicts;
        // never left in place without the engine knowing.
        let text = a.read("pages/P.md");
        let has_markers = text.lines().any(|l| l.starts_with("<<<<<<<"));
        assert!(
            !has_markers || report.marker_conflicts > 0,
            "{kind:?}: {text}"
        );
        if report.marker_conflicts > 0 {
            assert_eq!(*a.engine.state(), SyncState::Conflicted, "{kind:?}");
        }
    }
}

#[test]
fn status_message_variants_and_install_git_hint() {
    let (_w, a) = world(Kind::GixOnly);
    let mut s = a.engine.status();
    assert_eq!(s.backend, ActiveBackend::GixOnly);
    assert_eq!(s.user_message(), "Synced");
    s.ahead = 3;
    assert_eq!(s.user_message(), "3 local commits not synced");
    s.ahead = 1;
    assert_eq!(s.user_message(), "1 local commit not synced");
    assert_eq!(s.backend_hint(), None);
    s.state = SyncState::Error(SyncError::Auth("denied".into()));
    assert_eq!(s.backend_hint(), Some(INSTALL_GIT_HINT));
    assert!(s.can_retry());
    s.backend = ActiveBackend::Hybrid;
    assert_eq!(s.backend_hint(), None, "system git handles its own auth");
}
