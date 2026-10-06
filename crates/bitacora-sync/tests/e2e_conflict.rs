//! End-to-end conflict flow (BIT-T-0372): two clones of a temp bare remote edit the same graph
//! concurrently (one content conflict, a metadata-only change, a rename with an edit on the other
//! side and an asset collision). The first device to sync pushes; the second ends `Conflicted`
//! with exactly one open conflict, its work tree holds its own text, nothing is pushed; resolving
//! through the API pushes the merge and the first device converges to identical trees. Runs on
//! both backends. Also covers a user running `git pull` on the conflicted device.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use bitacora_sync::merge::{ConflictType, Resolution};
use bitacora_sync::state::SyncState;
use bitacora_testkit::git::git_raw;
use common::{Dev, KINDS, World, assert_history_legal, assert_no_markers};

const P: &str = "- Alpha is the first block of the page\n- Beta is the contested block\n- Gamma closes the page\n";
const M: &str = "- Topic block number one\n- Other block number two\n";
const OLD: &str = "- one is a fairly long first block\n- two is another reasonably long block\n- three closes the page nicely\n";

/// Every file of the work tree (outside `.git`) with its bytes.
fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if e.file_name() == ".git" {
                continue;
            }
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

fn put_bytes(d: &Dev, rel: &str, bytes: &[u8]) {
    let p = d.dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

fn setup(kind: common::Kind) -> (World, Dev, Dev) {
    let w = World::new(kind);
    let mut a = w.first_device(&[("pages/P.md", P), ("pages/M.md", M), ("pages/Old.md", OLD)]);
    assert_eq!(a.engine.sync_now(), SyncState::Idle);
    let mut b = w.clone_device();
    assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
    (w, a, b)
}

fn concurrent_edits(a: &Dev, b: &Dev) {
    // Device A.
    a.write(
        "pages/P.md",
        &P.replace("contested block", "contested block, A version"),
    );
    a.write(
        "pages/M.md",
        &M.replace(
            "- Topic block number one",
            "- Topic block number one\n  collapsed:: true",
        ),
    );
    std::fs::rename(a.dir.join("pages/Old.md"), a.dir.join("pages/New.md")).unwrap();
    put_bytes(a, "assets/img.png", b"A\0bytes");
    // Device B.
    b.write(
        "pages/P.md",
        &P.replace("contested block", "contested block, B version"),
    );
    b.write(
        "pages/M.md",
        &M.replace(
            "Other block number two",
            "Other block number two, edited by B",
        ),
    );
    b.write(
        "pages/Old.md",
        &OLD.replace("two is another", "two is yet another"),
    );
    put_bytes(b, "assets/img.png", b"B\0bytes");
}

#[test]
fn two_clones_conflict_resolve_and_converge() {
    for kind in KINDS {
        let (w, mut a, mut b) = setup(kind);
        concurrent_edits(&a, &b);

        // B syncs first and pushes.
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(b.head(), w.remote_head());
        let b_head = b.head();

        // A syncs second: exactly one open conflict, everything else auto-resolved.
        assert_eq!(a.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        let pending = a.engine.pending_merge().unwrap();
        assert_eq!(
            pending.conflicts.len(),
            1,
            "{kind:?}: {:?}",
            pending.conflicts
        );
        assert_eq!(pending.conflicts[0].kind, ConflictType::Content);
        assert_eq!(
            w.remote_head(),
            b_head,
            "{kind:?}: nothing was pushed while conflicted"
        );

        // The work tree holds A's text for the conflicting block and the merged rest.
        assert!(a.read("pages/P.md").contains("A version"), "{kind:?}");
        assert!(!a.read("pages/P.md").contains("B version"), "{kind:?}");
        assert_eq!(
            a.read("pages/M.md"),
            "- Topic block number one\n  collapsed:: true\n- Other block number two, edited by B\n",
            "{kind:?}"
        );
        assert!(!a.dir.join("pages/Old.md").exists(), "{kind:?}");
        assert_eq!(
            a.read("pages/New.md"),
            OLD.replace("two is another", "two is yet another"),
            "{kind:?}: B's edit applied at the renamed path"
        );
        // Asset collision: ours stays at the path, theirs is saved next to it.
        assert_eq!(
            std::fs::read(a.dir.join("assets/img.png")).unwrap(),
            b"A\0bytes"
        );
        let saved: Vec<_> = std::fs::read_dir(a.dir.join("assets"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("conflict"))
            .collect();
        assert_eq!(saved.len(), 1, "{kind:?}: {saved:?}");
        assert_eq!(
            std::fs::read(a.dir.join("assets").join(&saved[0])).unwrap(),
            b"B\0bytes"
        );
        assert_no_markers(&a.dir);

        // Resolve with B's text: the merge is committed and pushed.
        let out = a
            .engine
            .resolve_conflict(&pending.conflicts[0].id, Resolution::Theirs)
            .unwrap();
        assert_eq!(out.state, SyncState::Idle, "{kind:?}");
        assert!(a.read("pages/P.md").contains("B version"));
        assert_eq!(a.head(), w.remote_head(), "{kind:?}: resolve commit pushed");
        assert_no_markers(&a.dir);
        assert_history_legal(&a.engine);

        // B fetches the resolution and both trees are identical.
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(b.head(), a.head(), "{kind:?}");
        assert_eq!(tree(&a.dir), tree(&b.dir), "{kind:?}: trees converged");
        assert_no_markers(&b.dir);
    }
}

#[test]
fn a_users_git_pull_on_the_conflicted_device_is_cleaned_up_and_converges() {
    for kind in KINDS {
        let (w, mut a, mut b) = setup(kind);
        concurrent_edits(&a, &b);
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");

        // The user commits on A and runs `git pull` by hand: P.md stops unmerged.
        a.git(&["add", "-A"]);
        a.git(&["commit", "-qm", "alice edits"]);
        let out = git_raw(
            &a.dir,
            &["pull", "--no-rebase", "--no-edit", "origin", "main"],
        );
        assert!(
            !out.status.success(),
            "{kind:?}: the pull must stop with a conflict"
        );
        assert_eq!(a.engine.sync_now(), SyncState::Conflicted, "{kind:?}");
        assert!(
            !a.dir.join(".git/MERGE_HEAD").exists(),
            "{kind:?}: merge taken over"
        );
        assert_no_markers(&a.dir);
        let pending = a.engine.pending_merge().unwrap();
        assert_eq!(
            pending.conflicts.len(),
            1,
            "{kind:?}: {:?}",
            pending.conflicts
        );

        let out = a
            .engine
            .resolve_conflict(&pending.conflicts[0].id, Resolution::Ours)
            .unwrap();
        assert_eq!(out.state, SyncState::Idle, "{kind:?}");
        assert_eq!(a.head(), w.remote_head(), "{kind:?}");
        assert!(a.read("pages/P.md").contains("A version"));
        assert_eq!(b.engine.sync_now(), SyncState::Idle, "{kind:?}");
        assert_eq!(tree(&a.dir), tree(&b.dir), "{kind:?}: trees converged");
        assert_no_markers(&b.dir);
    }
}
