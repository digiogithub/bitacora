//! Two-clone convergence tests through a temp bare repository, run against both backends
//! (system git hybrid and gix-only). BIT-US-0045, BIT-T-0289, BIT-T-0376.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bitacora_sync::autocommit::CommitRequest;
use bitacora_sync::backend::{
    ActiveBackend, CommitInfo, CommitMessage, CommitOpts, FetchOutcome, GitBackend, GitError, Oid,
    PushOutcome, RepoStatus, TreeChange, TreeEdit, select_backend,
};
use bitacora_sync::commit_msg::parse_message;
use bitacora_sync::engine::{EngineConfig, SyncEngine, Timing};
use bitacora_sync::merge::ConflictType;
use bitacora_sync::onboarding::{RemoteState, enable_sync};
use bitacora_sync::state::{MemoryMergeStore, SyncError, SyncState};
use bitacora_sync::writer::testing::DirGraphWriter;
use bitacora_testkit::git::{commit_file, git};
use common::{Dev, KINDS, Kind, World, assert_history_legal, assert_no_markers, make_dev};

const PAGE: &str = "- Alpha\n- Beta\n- Gamma\n";

fn synced_pair(kind: Kind, files: &[(&str, &str)]) -> (World, Dev, Dev) {
    let w = World::new(kind);
    let mut a = w.first_device(files);
    assert_eq!(a.engine.sync_now(), SyncState::Idle);
    let mut b = w.clone_device();
    assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
    assert_eq!(a.head(), b.head());
    (w, a, b)
}

fn parents(d: &Dev, rev: &str) -> Vec<String> {
    d.git(&["log", "-1", "--format=%P", rev])
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

#[test]
fn fast_forward_updates_work_tree_without_a_commit() {
    for kind in KINDS {
        let (w, mut a, mut b) = synced_pair(kind, &[("pages/P.md", PAGE)]);
        a.write("pages/P.md", "- Alpha edited\n- Beta\n- Gamma\n");
        a.write("pages/New.md", "- new page\n");
        assert_eq!(a.engine.sync_now(), SyncState::Idle, "{kind:?}");

        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(b.head(), w.remote_head(), "{kind:?}");
        assert_eq!(
            b.git(&["rev-list", "--count", "HEAD"]),
            "2",
            "{kind:?}: fast-forward adds a's commit only"
        );
        assert_eq!(b.read("pages/P.md"), "- Alpha edited\n- Beta\n- Gamma\n");
        assert_eq!(b.read("pages/New.md"), "- new page\n");
        // The index was refreshed: nothing shows up as modified.
        assert_eq!(b.git(&["status", "--porcelain"]), "", "{kind:?}");
        // The merge only changed files through the writer.
        assert!(b.writer.applied().len() >= 2);
        assert_history_legal(&b.engine);
        assert!(b.engine.history().contains(&SyncState::FastForward));
    }
}

#[test]
fn deletions_and_renames_fast_forward() {
    for kind in KINDS {
        let (_w, mut a, mut b) = synced_pair(
            kind,
            &[
                ("pages/P.md", PAGE),
                ("pages/Old.md", "- old name\n"),
                ("pages/Gone.md", "- bye\n"),
            ],
        );
        std::fs::remove_file(a.dir.join("pages/Gone.md")).unwrap();
        std::fs::rename(a.dir.join("pages/Old.md"), a.dir.join("pages/Renamed.md")).unwrap();
        a.engine.sync_now();
        b.engine.sync_now();
        assert!(!b.dir.join("pages/Gone.md").exists(), "{kind:?}");
        assert!(!b.dir.join("pages/Old.md").exists(), "{kind:?}");
        assert_eq!(b.read("pages/Renamed.md"), "- old name\n");
        assert_eq!(b.git(&["status", "--porcelain"]), "");
    }
}

#[test]
fn divergence_on_different_blocks_creates_a_merge_commit() {
    for kind in KINDS {
        let (w, mut a, mut b) = synced_pair(kind, &[("pages/P.md", PAGE)]);
        a.write("pages/P.md", "- Alpha by alice\n- Beta\n- Gamma\n");
        b.write("pages/P.md", "- Alpha\n- Beta\n- Gamma by bob\n");
        assert_eq!(a.engine.sync_now(), SyncState::Idle, "{kind:?}");
        let alice_tip = a.head();

        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        let tip = b.head();
        assert_eq!(tip, w.remote_head(), "{kind:?}: merge commit was pushed");
        let ps = parents(&b, "HEAD");
        assert_eq!(ps.len(), 2, "{kind:?}: two-parent merge commit");
        assert_eq!(ps[1], alice_tip);
        let msg = parse_message(&b.git(&["log", "-1", "--format=%B"]));
        assert_eq!(msg.kind, Some(bitacora_sync::CommitKind::Merge));
        assert_eq!(msg.device.as_deref(), Some("bob"));
        assert_eq!(
            b.read("pages/P.md"),
            "- Alpha by alice\n- Beta\n- Gamma by bob\n"
        );
        assert_eq!(b.git(&["status", "--porcelain"]), "", "{kind:?}");
        assert_no_markers(&b.dir);

        // Alice converges by fast-forward.
        assert_eq!(a.engine.sync_now(), SyncState::Idle);
        assert_eq!(a.head(), tip);
        assert_eq!(a.read("pages/P.md"), b.read("pages/P.md"));
        assert_history_legal(&b.engine);
        assert!(b.engine.history().contains(&SyncState::Merging));
        assert_eq!(b.engine.status().ahead, 0);
    }
}

#[test]
fn metadata_only_differences_resolve_automatically() {
    for kind in KINDS {
        let base = "- Plan\n  id:: 66aa0000-0000-4000-8000-000000000001\n- Other\n";
        let (_w, mut a, mut b) = synced_pair(kind, &[("pages/P.md", base)]);
        // Alice collapses the block and adds a property; Bob edits its text.
        a.write(
            "pages/P.md",
            "- Plan\n  collapsed:: true\n  id:: 66aa0000-0000-4000-8000-000000000001\n- Other\n",
        );
        b.write(
            "pages/P.md",
            "- Plan for Q4\n  id:: 66aa0000-0000-4000-8000-000000000001\n- Other\n",
        );
        assert_eq!(a.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        let merged = b.read("pages/P.md");
        assert!(merged.contains("- Plan for Q4"), "{kind:?}: {merged}");
        assert!(merged.contains("collapsed:: true"), "{kind:?}: {merged}");
        assert!(merged.contains("id:: 66aa0000-0000-4000-8000-000000000001"));
        assert!(b.engine.pending_merge().is_none());
        assert_no_markers(&b.dir);
    }
}

#[test]
fn content_conflict_is_data_not_markers_and_nothing_is_pushed() {
    for kind in KINDS {
        let (w, mut a, mut b) = synced_pair(kind, &[("pages/P.md", PAGE)]);
        a.write("pages/P.md", "- Alpha\n- Beta from alice\n- Gamma\n");
        b.write("pages/P.md", "- Alpha\n- Beta from bob\n- Gamma\n");
        a.engine.sync_now();
        let remote = w.remote_head();

        assert_eq!(b.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        // The work tree keeps the user's version, with no markers anywhere.
        assert_eq!(
            b.read("pages/P.md"),
            "- Alpha\n- Beta from bob\n- Gamma\n",
            "{kind:?}"
        );
        assert_no_markers(&b.dir);
        // Nothing was pushed.
        assert_eq!(w.remote_head(), remote, "{kind:?}");
        // The conflict is persisted as data.
        let pending = b.engine.pending_merge().expect("pending merge");
        assert_eq!(
            pending.conflicts.len(),
            1,
            "{kind:?}: {:?}",
            pending.conflicts
        );
        let c = &pending.conflicts[0];
        assert_eq!(c.kind, ConflictType::Content);
        assert_eq!(c.path, "pages/P.md");
        assert_eq!(c.ours.as_deref(), Some("Beta from bob"));
        assert_eq!(c.theirs.as_deref(), Some("Beta from alice"));
        assert_eq!(c.base.as_deref(), Some("Beta"));
        assert_eq!(pending.theirs.as_hex(), remote);
        assert_eq!(b.engine.status().conflicts, 1);
        // Their commits are anchored by the pending-merge ref.
        let pm = b.git(&["rev-parse", "refs/bitacora/pending-merge"]);
        assert_eq!(pending.pending_commit.as_hex(), pm);
        assert_eq!(parents(&b, &pm).len(), 2);

        // Local edits continue to be committed, and a periodic sync keeps waiting.
        b.write("pages/Other.md", "- unrelated\n");
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        assert_eq!(w.remote_head(), remote, "{kind:?}: still nothing pushed");
        assert_eq!(
            b.git(&["status", "--porcelain"]),
            "",
            "{kind:?}: local work committed"
        );
        assert_no_markers(&b.dir);
        assert_history_legal(&b.engine);

        // The resolver (BIT-US-0054) writes the resolution through the writer; the engine
        // creates the resolve commit and pushes.
        b.write(
            "pages/P.md",
            "- Alpha\n- Beta from alice and bob\n- Gamma\n",
        );
        assert_eq!(
            b.engine.finish_pending_merge().unwrap(),
            SyncState::Idle,
            "{kind:?}"
        );
        assert!(b.engine.pending_merge().is_none());
        assert_eq!(b.head(), w.remote_head(), "{kind:?}");
        assert_eq!(parents(&b, "HEAD").len(), 2);
        assert!(parents(&b, "HEAD").contains(&remote));
        let msg = parse_message(&b.git(&["log", "-1", "--format=%B"]));
        assert_eq!(msg.kind, Some(bitacora_sync::CommitKind::Resolve));

        a.engine.sync_now();
        assert_eq!(
            a.read("pages/P.md"),
            "- Alpha\n- Beta from alice and bob\n- Gamma\n"
        );
        assert_eq!(a.read("pages/Other.md"), "- unrelated\n");
        assert_eq!(a.head(), b.head());
    }
}

#[test]
fn remote_moving_while_conflicted_recomputes_the_merge() {
    for kind in KINDS {
        let (w, mut a, mut b) = synced_pair(kind, &[("pages/P.md", PAGE)]);
        a.write("pages/P.md", "- Alpha\n- Beta from alice\n- Gamma\n");
        b.write("pages/P.md", "- Alpha\n- Beta from bob\n- Gamma\n");
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted);
        let first_theirs = b.engine.pending_merge().unwrap().theirs;

        a.write("pages/Extra.md", "- extra from alice\n");
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        let again = b.engine.pending_merge().unwrap();
        assert_ne!(
            again.theirs, first_theirs,
            "{kind:?}: recomputed against the new tip"
        );
        assert_eq!(again.theirs.as_hex(), w.remote_head());
        assert_eq!(b.read("pages/Extra.md"), "- extra from alice\n");
        assert_eq!(again.conflicts.len(), 1);
        assert_no_markers(&b.dir);
    }
}

#[test]
fn delete_versus_modify_at_file_level_is_a_conflict() {
    for kind in KINDS {
        let (_w, mut a, mut b) =
            synced_pair(kind, &[("pages/P.md", PAGE), ("pages/Q.md", "- q\n")]);
        std::fs::remove_file(a.dir.join("pages/Q.md")).unwrap();
        b.write("pages/Q.md", "- q edited by bob\n");
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        let c = &b.engine.pending_merge().unwrap().conflicts[0];
        assert_eq!(c.kind, ConflictType::FileDeleteVsModify);
        assert_eq!(
            b.read("pages/Q.md"),
            "- q edited by bob\n",
            "{kind:?}: user's file is kept"
        );
    }
}

#[test]
fn binary_and_whiteboard_collisions_keep_both_files() {
    for kind in KINDS {
        let (_w, mut a, mut b) = synced_pair(
            kind,
            &[
                ("pages/P.md", PAGE),
                ("assets/pic.bin", "base"),
                ("whiteboards/w.edn", "{:a 1}\n"),
            ],
        );
        a.write("assets/pic.bin", "alice");
        a.write("whiteboards/w.edn", "{:a 2}\n");
        b.write("assets/pic.bin", "bob");
        b.write("whiteboards/w.edn", "{:a 3}\n");
        a.engine.sync_now();
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(b.read("assets/pic.bin"), "bob");
        assert_eq!(b.read("whiteboards/w.edn"), "{:a 3}\n");
        let tree = b.git(&["ls-files"]);
        // Assets use the short sha of their commit; whiteboards name device and date.
        let asset: Vec<&str> = tree
            .lines()
            .filter(|l| l.starts_with("assets/pic (conflict-"))
            .collect();
        assert_eq!(asset.len(), 1, "{kind:?}: {tree}");
        assert_eq!(b.read(asset[0]), "alice");
        let board: Vec<&str> = tree
            .lines()
            .filter(|l| l.starts_with("whiteboards/w (conflict alice 20"))
            .collect();
        assert_eq!(board.len(), 1, "{kind:?}: {tree}");
        assert_eq!(b.read(board[0]), "{:a 2}\n");
    }
}

#[test]
fn unrelated_histories_merge_with_an_empty_base() {
    for kind in KINDS {
        let w = World::new(kind);
        // Device A publishes first.
        let mut a = w.first_device(&[
            ("journals/2026_10_06.md", "- alice morning\n"),
            ("pages/Home.md", "- home\n"),
        ]);
        a.engine.sync_now();
        // Device B has its own graph (no shared history) with an overlapping journal.
        let dir = w.path("b");
        for (p, c) in [
            ("journals/2026_10_06.md", "- bob evening\n"),
            ("pages/Home.md", "- home\n"),
            ("pages/Only B.md", "- b only\n"),
        ] {
            let full = dir.join(p);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, c).unwrap();
        }
        let cfg = common::onboarding_config(kind, "bob");
        let out = enable_sync(&dir, &w.url(), "main", &cfg).unwrap();
        let RemoteState::NeedsMerge { merge_base, .. } = out.remote else {
            panic!("{kind:?}: expected NeedsMerge, got {:?}", out.remote);
        };
        assert_eq!(merge_base, None);
        let mut b = make_dev(kind, &dir, "bob", |_| {});
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");

        let journal = b.read("journals/2026_10_06.md");
        assert!(
            journal.contains("alice morning") && journal.contains("bob evening"),
            "{kind:?}: {journal}"
        );
        assert_eq!(b.read("pages/Only B.md"), "- b only\n");
        assert_eq!(b.read("pages/Home.md"), "- home\n");
        assert_eq!(parents(&b, "HEAD").len(), 2);
        assert_eq!(b.head(), w.remote_head());
        assert_no_markers(&b.dir);
        // Alice receives the union.
        a.engine.sync_now();
        assert_eq!(a.read("journals/2026_10_06.md"), journal);
        assert_eq!(a.read("pages/Only B.md"), "- b only\n");
    }
}

#[test]
fn template_only_journal_is_treated_as_unchanged() {
    for kind in KINDS {
        let w = World::new(kind);
        let mut a = w.first_device(&[
            ("journals/2026_10_06.md", "- real notes\n"),
            ("pages/Home.md", "- h\n"),
        ]);
        a.engine.sync_now();
        let dir = w.path("b");
        for (p, c) in [
            ("journals/2026_10_06.md", "-\n"),
            ("pages/Home.md", "- h\n"),
        ] {
            let full = dir.join(p);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, c).unwrap();
        }
        enable_sync(
            &dir,
            &w.url(),
            "main",
            &common::onboarding_config(kind, "bob"),
        )
        .unwrap();
        let mut b = make_dev(kind, &dir, "bob", |_| {});
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(
            b.read("journals/2026_10_06.md"),
            "- real notes\n",
            "{kind:?}"
        );
    }
}

#[test]
fn offline_keeps_local_commits_and_backs_off_then_recovers() {
    for kind in KINDS {
        let (w, mut a, _b) = synced_pair(kind, &[("pages/P.md", PAGE)]);
        let moved = w.path("remote-away.git");
        std::fs::rename(&w.remote, &moved).unwrap();

        a.write("pages/One.md", "- 1\n");
        assert_eq!(a.engine.sync_now(), SyncState::Offline, "{kind:?}");
        assert!(a.engine.status().last_error.is_some());
        // Local commits continue while offline; the status reports them as unsynced.
        a.timing.advance(Duration::from_secs(40 * 60));
        a.write("pages/Two.md", "- 2\n");
        assert_eq!(a.engine.sync_now(), SyncState::Offline, "{kind:?}");
        assert!(
            a.engine.status().ahead >= 1,
            "{kind:?}: {:?}",
            a.engine.status()
        );
        assert_eq!(a.git(&["status", "--porcelain"]), "");

        // Back-off: the next retry is scheduled in the future and grows (30 s, then 60 s).
        let now = a.timing.now();
        let deadline = a.engine.next_deadline().unwrap();
        assert!(deadline > now, "{kind:?}");
        assert!(deadline - now <= Duration::from_secs(60), "{kind:?}");

        // Network returns: back-off resets and the sync completes.
        std::fs::rename(&moved, &w.remote).unwrap();
        a.engine.network_changed();
        assert_eq!(a.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(a.head(), w.remote_head());
        assert_eq!(a.engine.status().ahead, 0);
        assert_history_legal(&a.engine);
    }
}

#[test]
fn offline_backoff_schedule_is_30s_to_10min() {
    let w = World::new(Kind::Hybrid);
    let mut a = w.first_device(&[("pages/P.md", PAGE)]);
    a.engine.sync_now();
    std::fs::rename(&w.remote, w.path("gone.git")).unwrap();
    let mut delays = Vec::new();
    for _ in 0..7 {
        assert_eq!(a.engine.sync_now(), SyncState::Offline);
        let t = a.timing.now();
        let d = a.engine.next_deadline().unwrap() - t;
        // The retry deadline is the earliest of the retry and the periodic fetch.
        delays.push(d.as_secs().min(120));
        a.timing.advance(Duration::from_secs(1));
    }
    assert!(delays[0] <= 30 && delays[0] > 0, "{delays:?}");
    // Capped delays never exceed 10 minutes.
    assert!(delays.iter().all(|d| *d <= 600));
}

#[test]
fn writer_busy_defers_the_sync_until_the_transaction_ends() {
    for kind in KINDS {
        let (w, mut a, _b) = synced_pair(kind, &[("pages/P.md", PAGE)]);
        a.write("pages/One.md", "- 1\n");
        a.writer.set_busy(true);
        let remote = w.remote_head();
        assert_eq!(a.engine.sync_now(), SyncState::Dirty, "{kind:?}");
        assert_eq!(
            w.remote_head(),
            remote,
            "{kind:?}: nothing ran while a transaction was pending"
        );
        assert_eq!(
            a.git(&["status", "--porcelain"]).lines().count(),
            1,
            "{kind:?}: not committed either"
        );
        a.writer.set_busy(false);
        assert_eq!(a.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(a.head(), w.remote_head());
    }
}

/// A backend whose `push` loses the race `n` times: another device pushes first.
struct RacingBackend {
    inner: Box<dyn GitBackend>,
    races_left: Mutex<u32>,
    racer: Mutex<Box<dyn FnMut() + Send>>,
    pushes: Mutex<u32>,
}

impl GitBackend for RacingBackend {
    fn fetch(&self, r: &str, b: &str) -> Result<FetchOutcome, GitError> {
        self.inner.fetch(r, b)
    }
    fn push(&self, r: &str, b: &str) -> Result<PushOutcome, GitError> {
        *self.pushes.lock().unwrap() += 1;
        let mut left = self.races_left.lock().unwrap();
        if *left > 0 {
            *left -= 1;
            (self.racer.lock().unwrap())();
            return Err(GitError::NonFastForward);
        }
        self.inner.push(r, b)
    }
    fn merge_base(&self, a: &Oid, b: &Oid) -> Result<Option<Oid>, GitError> {
        self.inner.merge_base(a, b)
    }
    fn read_blob(&self, c: &Oid, p: &str) -> Result<Option<Vec<u8>>, GitError> {
        self.inner.read_blob(c, p)
    }
    fn diff_trees(&self, a: &Oid, b: &Oid) -> Result<Vec<TreeChange>, GitError> {
        self.inner.diff_trees(a, b)
    }
    fn write_tree(&self, b: &Oid, e: &[TreeEdit]) -> Result<Oid, GitError> {
        self.inner.write_tree(b, e)
    }
    fn commit(&self, m: &CommitMessage, o: CommitOpts) -> Result<Oid, GitError> {
        self.inner.commit(m, o)
    }
    fn commit_tree(&self, t: &Oid, p: &[Oid], m: &CommitMessage) -> Result<Oid, GitError> {
        self.inner.commit_tree(t, p, m)
    }
    fn update_ref(&self, n: &str, new: &Oid, old: Option<&Oid>) -> Result<(), GitError> {
        self.inner.update_ref(n, new, old)
    }
    fn status(&self) -> Result<RepoStatus, GitError> {
        self.inner.status()
    }
    fn resolve_ref(&self, n: &str) -> Result<Option<Oid>, GitError> {
        self.inner.resolve_ref(n)
    }
    fn commit_info(&self, c: &Oid) -> Result<CommitInfo, GitError> {
        self.inner.commit_info(c)
    }
    fn reset_index(&self, c: &Oid) -> Result<(), GitError> {
        self.inner.reset_index(c)
    }
    fn kind(&self) -> ActiveBackend {
        self.inner.kind()
    }
}

/// Engine, race counter, clock and the heads the racer pushed.
type Racing = (
    SyncEngine,
    Arc<Mutex<u32>>,
    Arc<common::ManualTiming>,
    Arc<Mutex<Vec<String>>>,
);

fn racing_engine(w: &World, a: &Dev, races: u32) -> Racing {
    let cfg = common::onboarding_config(w.kind, "alice");
    let inner = select_backend(&cfg.detection, &a.dir, Default::default()).unwrap();
    let racer_dir = w.path("racer");
    let url = w.url();
    bitacora_testkit::git::clone_to(&url, &racer_dir);
    let counter = Arc::new(Mutex::new(0u32));
    let c2 = counter.clone();
    let heads = Arc::new(Mutex::new(Vec::new()));
    let h2 = heads.clone();
    let racer = move || {
        let mut n = c2.lock().unwrap();
        *n += 1;
        git(
            &racer_dir,
            &["pull", "-q", "--no-rebase", "--no-edit", "origin", "main"],
        );
        commit_file(
            &racer_dir,
            &format!("pages/Racer{n}.md"),
            "- race\n",
            "racer commit",
        );
        git(&racer_dir, &["push", "-q", "origin", "main"]);
        h2.lock()
            .unwrap()
            .push(git(&racer_dir, &["rev-parse", "HEAD"]));
    };
    let backend = RacingBackend {
        inner,
        races_left: Mutex::new(races),
        racer: Mutex::new(Box::new(racer)),
        pushes: Mutex::new(0),
    };
    let timing = common::ManualTiming::new();
    let engine = SyncEngine::new(
        Box::new(backend),
        Arc::new(DirGraphWriter::new(&a.dir)),
        Box::new(MemoryMergeStore::new()),
        EngineConfig::new(&a.dir, "alice", "main"),
        timing.clone(),
    );
    (engine, counter, timing, heads)
}

#[test]
fn push_race_retries_with_jitter_then_succeeds() {
    for kind in KINDS {
        let w = World::new(kind);
        let mut a = w.first_device(&[("pages/P.md", PAGE)]);
        a.engine.sync_now();
        a.write("pages/Mine.md", "- mine\n");
        let (mut engine, counter, timing, _) = racing_engine(&w, &a, 2);
        assert_eq!(engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(*counter.lock().unwrap(), 2);
        // Each rejected push re-fetches after a 1-8 s jitter sleep.
        let sleeps = timing.recorded_sleeps();
        assert_eq!(sleeps.len(), 2, "{kind:?}");
        assert!(
            sleeps
                .iter()
                .all(|s| *s >= Duration::from_secs(1) && *s <= Duration::from_secs(8))
        );
        assert_eq!(
            git(&a.dir, &["rev-parse", "HEAD"]),
            w.remote_head(),
            "{kind:?}"
        );
        let files = git(&a.dir, &["ls-files"]);
        assert!(
            files.contains("Racer1.md") && files.contains("Racer2.md") && files.contains("Mine.md")
        );
        assert_no_markers(&a.dir);
    }
}

#[test]
fn push_race_five_times_ends_in_push_rejected_loop_without_forcing() {
    for kind in KINDS {
        let w = World::new(kind);
        let mut a = w.first_device(&[("pages/P.md", PAGE)]);
        a.engine.sync_now();
        a.write("pages/Mine.md", "- mine\n");
        let (mut engine, counter, timing, heads) = racing_engine(&w, &a, 5);
        let state = engine.sync_now();
        assert_eq!(
            state,
            SyncState::Error(SyncError::PushRejectedLoop),
            "{kind:?}"
        );
        assert_eq!(*counter.lock().unwrap(), 5);
        assert_eq!(
            timing.recorded_sleeps().len(),
            4,
            "{kind:?}: sleeps between the 5 attempts"
        );
        // No force push: the remote tip is exactly the last racer commit, not ours.
        assert_eq!(w.remote_head(), *heads.lock().unwrap().last().unwrap());
        assert!(engine.status().last_error.is_some());
        // Local work is intact and merged with everything fetched so far.
        assert_eq!(git(&a.dir, &["status", "--porcelain"]), "");
        assert_no_markers(&a.dir);
        // An explicit retry works once the races stop.
        assert_eq!(engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(git(&a.dir, &["rev-parse", "HEAD"]), w.remote_head());
    }
}

#[test]
fn external_merge_in_progress_is_reported_not_touched() {
    let (_w, mut a, _b) = synced_pair(Kind::Hybrid, &[("pages/P.md", PAGE)]);
    std::fs::write(a.dir.join(".git/MERGE_HEAD"), format!("{}\n", a.head())).unwrap();
    a.write("pages/One.md", "- 1\n");
    assert_eq!(
        a.engine.sync_now(),
        SyncState::Error(SyncError::ExternalOperationInProgress)
    );
    assert!(
        a.git(&["status", "--porcelain"]).contains("One.md"),
        "nothing committed"
    );
}

#[test]
fn disabled_engine_does_nothing() {
    let (w, mut a, _b) = synced_pair(Kind::Hybrid, &[("pages/P.md", PAGE)]);
    a.engine.disable();
    a.write("pages/One.md", "- 1\n");
    let remote = w.remote_head();
    assert_eq!(a.engine.sync_now(), SyncState::Disabled);
    assert_eq!(w.remote_head(), remote);
    assert!(a.engine.next_deadline().is_none());
    a.engine.enable();
    assert_eq!(a.engine.sync_now(), SyncState::Idle);
    let _ = a.engine.commit_now(&CommitRequest::auto());
}

#[test]
fn three_way_convergence_after_interleaved_edits() {
    for kind in KINDS {
        let (_w, mut a, mut b) = synced_pair(
            kind,
            &[
                ("pages/P.md", "- A1\n- B1\n- C1\n- D1\n"),
                ("journals/2026_10_07.md", "- j\n"),
            ],
        );
        for round in 0..3 {
            let mut la: Vec<String> = a.read("pages/P.md").lines().map(str::to_string).collect();
            la[0] = format!("- A{round}x");
            a.write("pages/P.md", &(la.join("\n") + "\n"));
            let mut lb: Vec<String> = b.read("pages/P.md").lines().map(str::to_string).collect();
            let last = lb.len() - 1;
            lb[last] = format!("- D{round}y");
            b.write("pages/P.md", &(lb.join("\n") + "\n"));
            b.write(&format!("pages/B{round}.md"), "- b\n");
            a.write(&format!("pages/A{round}.md"), "- a\n");
            a.engine.sync_now();
            b.engine.sync_now();
            a.engine.sync_now();
            assert_eq!(a.head(), b.head(), "{kind:?} round {round}");
            assert_eq!(a.read("pages/P.md"), b.read("pages/P.md"));
            assert_eq!(
                a.read("pages/P.md").lines().next().unwrap(),
                format!("- A{round}x")
            );
            assert_eq!(
                a.read("pages/P.md").lines().last().unwrap(),
                format!("- D{round}y")
            );
            assert_no_markers(&a.dir);
            assert_no_markers(&b.dir);
        }
        assert_history_legal(&a.engine);
        assert_history_legal(&b.engine);
    }
}

#[test]
fn agent_writes_are_committed_as_kind_agent_with_the_client_trailer() {
    for kind in KINDS {
        let (_w, mut a, _b) = synced_pair(kind, &[("pages/P.md", PAGE)]);
        a.write("pages/P.md", "- Alpha by agent\n- Beta\n- Gamma\n");
        a.engine.note_agent_write("claude-desktop");
        assert_eq!(a.engine.sync_now(), SyncState::Idle, "{kind:?}");
        let msg = a.git(&["log", "-1", "--format=%B", "HEAD"]);
        let parsed = parse_message(&msg);
        assert_eq!(
            parsed.kind,
            Some(bitacora_sync::CommitKind::Agent),
            "{kind:?}: {msg}"
        );
        assert!(
            msg.contains("Bitacora-Agent: claude-desktop"),
            "{kind:?}: {msg}"
        );

        // A later user edit is an ordinary auto commit again.
        a.write("pages/P.md", "- Alpha by agent\n- Beta by user\n- Gamma\n");
        assert_eq!(a.engine.sync_now(), SyncState::Idle, "{kind:?}");
        let msg = a.git(&["log", "-1", "--format=%B", "HEAD"]);
        assert_eq!(
            parse_message(&msg).kind,
            Some(bitacora_sync::CommitKind::Auto),
            "{kind:?}: {msg}"
        );
    }
}
