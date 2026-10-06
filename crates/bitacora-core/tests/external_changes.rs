//! External changes to loaded pages: reload with stable block ids, 3-way merge into dirty pages
//! and the conflict notice (BIT-US-0068, BIT-US-0069, BIT-US-0070).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::SystemTime;

use bitacora_core::editor::{
    BlockId, Cmd, DiffKind, ExternalEvent, ExternalOutcome, FileStore, MemStore, Workspace, align,
    diff_blocks,
};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;

fn key() -> PageKey {
    PageKey::from_title("p")
}

fn path() -> GraphPath {
    GraphPath::new("pages/p.md").expect("path")
}

fn open(src: &str) -> (Workspace, MemStore) {
    let mut ws = Workspace::new();
    ws.load_page(key(), "p", Some(path()), src.as_bytes());
    let mut store = MemStore::default();
    store.files.insert(path(), src.as_bytes().to_vec());
    (ws, store)
}

fn ids(ws: &Workspace) -> Vec<BlockId> {
    ws.page(&key()).unwrap().dfs()
}

fn texts(ws: &Workspace) -> Vec<String> {
    let p = ws.page(&key()).unwrap();
    p.dfs()
        .iter()
        .map(|i| p.block(*i).unwrap().text.clone())
        .collect()
}

fn set(ws: &mut Workspace, id: BlockId, text: &str) {
    ws.run(
        "edit",
        &Cmd::SetText {
            id,
            text: text.to_owned(),
        },
    )
    .unwrap();
}

const BASE: &str = "- alpha one two three\n- beta one two three\n- gamma one two three\n";

#[test]
fn align_matches_uuid_first_then_content() {
    let old = b"- a id\n  id:: 11111111-1111-1111-1111-111111111111\n- b\n";
    let new = b"- z first\n- b\n- a id changed\n  id:: 11111111-1111-1111-1111-111111111111\n";
    assert_eq!(align(old, new), vec![Some(2), Some(1)]);
}

#[test]
fn clean_reload_keeps_ids_of_surviving_blocks() {
    let (mut ws, _) = open(BASE);
    let before = ids(&ws);
    let theirs = "- alpha one two three\n- beta one two three EDITED\n- gamma one two three\n- delta new block here\n";
    let out = ws.apply_external(&key(), theirs.as_bytes());
    let ExternalOutcome::Reloaded(r) = out else {
        panic!("{out:?}")
    };
    let after = ids(&ws);
    assert_eq!(&after[..3], &before[..], "ids are stable");
    assert_eq!(r.changed, vec![before[1]]);
    assert_eq!(r.added.len(), 1);
    assert!(r.removed.is_empty());
    assert_eq!(texts(&ws)[1], "beta one two three EDITED");
    assert!(!ws.page(&key()).unwrap().needs_write());
    assert!(matches!(
        ws.take_external_events().as_slice(),
        [ExternalEvent::Reloaded(_)]
    ));
}

#[test]
fn removed_blocks_are_reported() {
    let (mut ws, _) = open(BASE);
    let before = ids(&ws);
    let ExternalOutcome::Reloaded(r) =
        ws.apply_external(&key(), b"- alpha one two three\n- gamma one two three\n")
    else {
        panic!()
    };
    assert_eq!(r.removed, vec![before[1]]);
}

#[test]
fn identical_bytes_are_unchanged() {
    let (mut ws, _) = open(BASE);
    assert!(matches!(
        ws.apply_external(&key(), BASE.as_bytes()),
        ExternalOutcome::Unchanged
    ));
}

#[test]
fn editing_block_change_is_reported_and_id_stays() {
    let (mut ws, _) = open(BASE);
    let id = ids(&ws)[1];
    ws.set_editing_block(Some(id));
    ws.apply_external(
        &key(),
        b"- alpha one two three\n- beta one two three EDITED\n- gamma one two three\n",
    );
    let evs = ws.take_external_events();
    let c = evs
        .iter()
        .find_map(|e| match e {
            ExternalEvent::EditingBlockChanged(c) => Some(c.clone()),
            _ => None,
        })
        .expect("editing event");
    assert_eq!(c.block, id);
    assert_eq!(c.mine, "beta one two three");
    assert_eq!(c.disk, "beta one two three EDITED");
    assert!(ws.page(&key()).unwrap().block(id).is_some());
}

#[test]
fn disjoint_edits_merge_and_flush() {
    let (mut ws, mut store) = open(BASE);
    let first = ids(&ws)[0];
    set(&mut ws, first, "alpha one two three MINE");
    let theirs = "- alpha one two three\n- beta one two three\n- gamma one two three THEIRS\n";
    store.files.insert(path(), theirs.as_bytes().to_vec());

    let out = ws.apply_external(&key(), theirs.as_bytes());
    assert!(matches!(out, ExternalOutcome::Merged { .. }), "{out:?}");
    assert_eq!(ids(&ws)[0], first, "ids stable across the merge");
    assert_eq!(
        texts(&ws),
        vec![
            "alpha one two three MINE",
            "beta one two three",
            "gamma one two three THEIRS"
        ]
    );
    assert!(ws.page(&key()).unwrap().needs_write());
    let r = ws.flush(&mut store);
    assert!(r.is_complete(), "{r:?}");
    assert_eq!(
        String::from_utf8(store.files[&path()].clone()).unwrap(),
        "- alpha one two three MINE\n- beta one two three\n- gamma one two three THEIRS\n"
    );
}

#[test]
fn flush_merges_when_the_file_moved_on() {
    // No watcher event arrived: the pre-write check finds the changed file and merges.
    let (mut ws, mut store) = open(BASE);
    let first = ids(&ws)[0];
    set(&mut ws, first, "alpha one two three MINE");
    let theirs = "- alpha one two three\n- beta one two three THEIRS\n- gamma one two three\n";
    store.files.insert(path(), theirs.as_bytes().to_vec());
    let r = ws.flush(&mut store);
    assert!(r.is_complete(), "{r:?}");
    assert_eq!(
        String::from_utf8(store.files[&path()].clone()).unwrap(),
        "- alpha one two three MINE\n- beta one two three THEIRS\n- gamma one two three\n"
    );
    assert!(
        ws.take_external_events()
            .iter()
            .any(|e| matches!(e, ExternalEvent::Merged(_)))
    );
}

#[test]
fn same_block_conflict_keeps_ours_and_never_writes() {
    let (mut ws, mut store) = open(BASE);
    let first = ids(&ws)[0];
    set(&mut ws, first, "alpha one two three MINE");
    let theirs = "- alpha one two three DISK\n- beta one two three\n- gamma one two three\n";
    store.files.insert(path(), theirs.as_bytes().to_vec());

    let ExternalOutcome::Conflict(n) = ws.apply_external(&key(), theirs.as_bytes()) else {
        panic!("expected conflict")
    };
    assert!(n.base_available);
    assert!(!n.conflicts.is_empty());
    assert_eq!(n.diff.len(), 1);
    assert_eq!(n.diff[0].kind, DiffKind::Changed);
    assert_eq!(
        n.diff[0].mine.as_deref(),
        Some("- alpha one two three MINE")
    );
    assert_eq!(texts(&ws)[0], "alpha one two three MINE");
    assert!(ws.conflict(&key()).is_some());

    // Writes are stopped, the file is untouched and carries no markers.
    let r = ws.flush(&mut store);
    assert_eq!(r.conflicts, vec![key()]);
    assert_eq!(store.files[&path()], theirs.as_bytes());
    assert!(!String::from_utf8_lossy(&store.files[&path()]).contains("<<<<<<<"));

    // Keep mine: the disk version is backed up first, then ours is written.
    let r = ws.resolve_keep_mine(&key(), &mut store, SystemTime::now());
    assert!(r.is_complete(), "{r:?}");
    assert!(!r.backups.is_empty());
    assert_eq!(
        String::from_utf8(store.files[&path()].clone()).unwrap(),
        "- alpha one two three MINE\n- beta one two three\n- gamma one two three\n"
    );
    assert!(ws.conflict(&key()).is_none());
}

#[test]
fn take_disk_resolves_a_conflict_keeping_ids() {
    let (mut ws, mut store) = open(BASE);
    let before = ids(&ws);
    set(&mut ws, before[0], "alpha one two three MINE");
    let theirs = "- alpha one two three DISK\n- beta one two three\n- gamma one two three\n";
    store.files.insert(path(), theirs.as_bytes().to_vec());
    assert!(matches!(
        ws.apply_external(&key(), theirs.as_bytes()),
        ExternalOutcome::Conflict(_)
    ));
    let mut report = Default::default();
    ws.take_disk(&key(), &mut store, SystemTime::now(), &mut report);
    assert_eq!(texts(&ws)[0], "alpha one two three DISK");
    assert_eq!(ids(&ws), before);
    assert!(ws.conflict(&key()).is_none());
    assert!(!ws.page(&key()).unwrap().needs_write());
    assert_eq!(report.backups.len(), 1, "our version was backed up");
}

#[test]
fn page_without_base_goes_to_a_two_way_diff() {
    // A page we never wrote (no merge base, ADR-017) with unsaved edits.
    let mut ws = Workspace::new();
    let mut page = bitacora_core::editor::Page::empty(key(), "p", Some(path()));
    page.dirty = true;
    ws.insert_page(page);
    let id = ws.alloc_id();
    ws.commit(
        "add",
        vec![bitacora_core::editor::Op::InsertSubtree {
            page: key(),
            parent: None,
            index: 0,
            subtree: bitacora_core::editor::Subtree::new(id, "mine only"),
        }],
    )
    .unwrap();
    let ExternalOutcome::Conflict(n) = ws.apply_external(&key(), b"- theirs only\n") else {
        panic!("expected conflict")
    };
    assert!(!n.base_available);
    let kinds: Vec<_> = n.diff.iter().map(|d| d.kind).collect();
    assert!(kinds.contains(&DiffKind::OnlyMine) && kinds.contains(&DiffKind::OnlyDisk));
}

#[test]
fn diff_blocks_is_empty_for_equal_pages() {
    assert!(diff_blocks(BASE.as_bytes(), BASE.as_bytes()).is_empty());
}

#[test]
fn delete_vs_modify_is_a_conflict() {
    let (mut ws, mut store) = open(BASE);
    let id = ids(&ws)[1];
    set(&mut ws, id, "beta one two three MINE");
    let theirs = "- alpha one two three\n- gamma one two three\n";
    store.files.insert(path(), theirs.as_bytes().to_vec());
    assert!(matches!(
        ws.apply_external(&key(), theirs.as_bytes()),
        ExternalOutcome::Conflict(_)
    ));
    assert_eq!(texts(&ws)[1], "beta one two three MINE");
    let r = ws.flush(&mut store);
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(store.read(&path()).unwrap().unwrap(), theirs.as_bytes());
}

#[test]
fn queue_reports_editing_block_conflicts_and_refuses_mcp_writes() {
    use bitacora_core::queue::{
        CommandQueue, QueueConfig, QueueError, QueueEvent, Request, Response, Source,
    };
    use std::sync::{Arc, Mutex};

    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&seen);
    let (ws, store) = open(BASE);
    let cfg = QueueConfig {
        observers: vec![Box::new(move |e: &QueueEvent| {
            let tag = match e {
                QueueEvent::EditingBlockChanged(_) => "editing",
                QueueEvent::PageConflicted(_) => "conflicted",
                QueueEvent::PageMerged(_) => "merged",
                QueueEvent::PageReloaded(_) => "reloaded",
                _ => return,
            };
            sink.lock().unwrap().push(tag.to_owned());
        })],
        debounce: None,
        ..QueueConfig::default()
    };
    let (q, join) = CommandQueue::spawn(ws, Box::new(store), cfg);
    let snap = q.snapshot(&key()).unwrap();
    let (a, b) = (snap.blocks[0].id, snap.blocks[1].id);

    q.set_editing_block(Some(b));
    let r = q
        .execute(
            Source::External,
            Request::ExternalChange {
                key: key(),
                bytes:
                    b"- alpha one two three\n- beta one two three EDITED\n- gamma one two three\n"
                        .to_vec(),
            },
        )
        .unwrap();
    assert!(matches!(
        r,
        Response::External(ExternalOutcome::Reloaded(_))
    ));
    assert_eq!(q.snapshot(&key()).unwrap().blocks[1].id, b);

    // Conflict: refuse MCP writes, keep UI writes possible.
    q.run(
        Source::Ui,
        "edit",
        Cmd::SetText {
            id: a,
            text: "alpha MINE one two three".into(),
        },
    )
    .unwrap();
    q.execute(
        Source::External,
        Request::ExternalChange {
            key: key(),
            bytes: b"- alpha THEIRS one two three\n- beta one two three EDITED\n- gamma one two three\n"
                .to_vec(),
        },
    )
    .unwrap();
    assert!(q.conflict(&key()).is_some());
    let err = q
        .run(
            Source::Mcp,
            "mcp edit",
            Cmd::SetText {
                id: b,
                text: "beta by mcp one two".into(),
            },
        )
        .unwrap_err();
    assert!(matches!(err, QueueError::PageConflicted(_)), "{err:?}");
    assert_eq!(
        q.snapshot(&key()).unwrap().blocks[1].text,
        "beta one two three EDITED"
    );
    drop(join.shutdown());
    let tags = seen.lock().unwrap().clone();
    assert_eq!(tags, vec!["editing", "reloaded", "conflicted"]);
}
