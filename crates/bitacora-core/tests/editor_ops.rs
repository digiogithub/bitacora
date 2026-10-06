//! Op/transaction tests for the editing core (BIT-US-0029; BIT-SP-0004.R4, R5).
//!
//! Written from the documented rules (ADR-015); the central property is that applying a
//! transaction and then its inverse restores the exact serialized bytes.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bitacora_core::editor::{
    Cmd, CommitError, InvariantError, Op, OpError, Position, Subtree, Target, Workspace,
};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;

const SRC: &str = "title:: T\n\n- a\n\t- a1\n\t- a2\n- b\n\t- b1\n- c\n";

fn key() -> PageKey {
    PageKey::from_title("p")
}

fn open(src: &str) -> Workspace {
    let mut ws = Workspace::new();
    ws.load_page(
        key(),
        "p",
        GraphPath::new("pages/p.md").ok(),
        src.as_bytes(),
    );
    ws
}

fn ser(ws: &Workspace) -> String {
    String::from_utf8(ws.page(&key()).expect("page").serialize()).expect("utf8")
}

/// Block id by text prefix.
fn id(ws: &Workspace, text: &str) -> bitacora_core::editor::BlockId {
    ws.page(&key())
        .expect("page")
        .blocks
        .values()
        .find(|b| b.text == text)
        .unwrap_or_else(|| panic!("no block {text:?}"))
        .id
}

fn assert_undo_restores(ws: &mut Workspace, label: &'static str, cmd: &Cmd) -> String {
    let before = ser(ws);
    let tx = ws.run(label, cmd).expect("commit");
    let after = ser(ws);
    let undo = ws.undo(&tx).expect("undo");
    assert_eq!(ser(ws), before, "undo of {label} must restore the bytes");
    ws.undo(&undo).expect("redo");
    assert_eq!(ser(ws), after, "redo of {label}");
    after
}

#[test]
fn untouched_page_serializes_identically_and_blocks_are_clean() {
    let ws = open(SRC);
    assert_eq!(ser(&ws), SRC);
    let p = ws.page(&key()).expect("page");
    assert!(p.dfs().iter().all(|b| p.is_clean(*b)));
    assert!(ws.dirty_pages().is_empty());
}

#[test]
fn set_text_rewrites_only_that_block() {
    let mut ws = open(SRC);
    let a1 = id(&ws, "a1");
    let after = assert_undo_restores(
        &mut ws,
        "Set text",
        &Cmd::SetText {
            id: a1,
            text: "A one".into(),
        },
    );
    assert_eq!(
        after,
        "title:: T\n\n- a\n\t- A one\n\t- a2\n- b\n\t- b1\n- c\n"
    );
}

#[test]
fn same_depth_move_keeps_blocks_clean_and_moves_bytes() {
    let mut ws = open(SRC);
    let (a, c) = (id(&ws, "a"), id(&ws, "c"));
    let after = assert_undo_restores(
        &mut ws,
        "Move",
        &Cmd::MoveBlocks {
            ids: vec![c],
            target: Target::Before(a),
        },
    );
    assert_eq!(
        after,
        "title:: T\n\n- c\n- a\n\t- a1\n\t- a2\n- b\n\t- b1\n"
    );
    ws.run(
        "Move",
        &Cmd::MoveBlocks {
            ids: vec![c],
            target: Target::After(id(&ws, "b")),
        },
    )
    .expect("move");
    let p = ws.page(&key()).expect("page");
    assert!(
        p.dfs().iter().all(|b| p.is_clean(*b)),
        "same-depth moves stay clean"
    );
}

#[test]
fn indent_and_outdent_direct_mode() {
    let mut ws = open(SRC);
    let b = id(&ws, "b");
    let after = assert_undo_restores(&mut ws, "Indent", &Cmd::Indent { ids: vec![b] });
    assert_eq!(
        after,
        "title:: T\n\n- a\n\t- a1\n\t- a2\n\t- b\n\t\t- b1\n- c\n"
    );

    // Outdent a1: following sibling a2 becomes its child.
    let mut ws = open(SRC);
    let a1 = id(&ws, "a1");
    let after = assert_undo_restores(&mut ws, "Outdent", &Cmd::Outdent { ids: vec![a1] });
    assert_eq!(after, "title:: T\n\n- a\n- a1\n\t- a2\n- b\n\t- b1\n- c\n");
}

#[test]
fn indent_expands_new_parent() {
    let mut ws = open(
        "- a\n  collapsed:: true\n\t- x\n- b\n"
            .replace('\t', "  ")
            .as_str(),
    );
    let b = id(&ws, "b");
    ws.run("Indent", &Cmd::Indent { ids: vec![b] })
        .expect("indent");
    let a = ws
        .page(&key())
        .expect("p")
        .blocks
        .values()
        .find(|x| x.text.starts_with('a'))
        .expect("a");
    assert!(!a.text.contains("collapsed"), "{}", a.text);
}

#[test]
fn insert_delete_and_inverse() {
    let mut ws = open(SRC);
    let a = id(&ws, "a");
    let after = assert_undo_restores(
        &mut ws,
        "Insert",
        &Cmd::InsertSibling {
            after: a,
            text: "new\nsecond line".into(),
        },
    );
    assert_eq!(
        after,
        "title:: T\n\n- a\n\t- a1\n\t- a2\n- new\n  second line\n- b\n\t- b1\n- c\n"
    );
    let mut ws = open(SRC);
    let b = id(&ws, "b");
    let after = assert_undo_restores(&mut ws, "Delete", &Cmd::DeleteBlocks { ids: vec![b] });
    assert_eq!(after, "title:: T\n\n- a\n\t- a1\n\t- a2\n- c\n");
}

#[test]
fn collapse_and_property_edits_are_single_block_text_edits() {
    let mut ws = open(SRC);
    let a = id(&ws, "a");
    let after = assert_undo_restores(
        &mut ws,
        "Collapse",
        &Cmd::SetCollapsed {
            ids: vec![a],
            collapsed: true,
        },
    );
    assert!(after.contains("- a\n  collapsed:: true\n\t- a1"), "{after}");
    let b = id(&ws, "b");
    let after = assert_undo_restores(
        &mut ws,
        "Prop",
        &Cmd::SetProperty {
            id: b,
            key: "status".into(),
            value: "x".into(),
        },
    );
    assert!(after.contains("- b\n  status:: x\n"), "{after}");
}

#[test]
fn edit_text_and_set_preamble_ops() {
    let mut ws = open(SRC);
    let a = id(&ws, "a");
    let mut ops = vec![
        Op::EditText {
            id: a,
            range: 0..1,
            removed: "a".into(),
            inserted: "ä".into(),
        },
        Op::SetPreamble {
            page: key(),
            before: Some("title:: T".into()),
            after: Some("title:: T\nalias:: q".into()),
        },
    ];
    for op in &mut ops {
        op.apply(&mut ws).expect("apply");
    }
    assert!(
        ser(&ws).starts_with("title:: T\nalias:: q\n\n- ä\n\t- a1"),
        "{}",
        ser(&ws)
    );
    for op in ops.iter().rev() {
        op.inverse()
            .expect("inv")
            .apply(&mut ws)
            .expect("apply inverse");
    }
    assert_eq!(ser(&ws), SRC);
}

#[test]
fn stale_ops_are_rejected_and_roll_back() {
    let mut ws = open(SRC);
    let (a, b) = (id(&ws, "a"), id(&ws, "b"));
    let err = ws
        .commit(
            "x",
            vec![
                Op::SetText {
                    id: a,
                    before: "a".into(),
                    after: "z".into(),
                },
                Op::SetText {
                    id: b,
                    before: "WRONG".into(),
                    after: "z".into(),
                },
            ],
        )
        .expect_err("stale");
    assert!(matches!(err, CommitError::Op { index: 1, .. }), "{err:?}");
    assert_eq!(ser(&ws), SRC, "first op rolled back");
}

#[test]
fn move_into_own_descendant_is_rejected() {
    let mut ws = open(SRC);
    let (a, a1) = (id(&ws, "a"), id(&ws, "a1"));
    let err = ws
        .commit(
            "x",
            vec![Op::Move {
                id: a,
                from: None,
                to: Position {
                    page: key(),
                    parent: Some(a1),
                    index: 0,
                },
            }],
        )
        .expect_err("cycle");
    assert_eq!(
        err,
        CommitError::Op {
            index: 0,
            source: OpError::MoveIntoDescendant(a)
        }
    );
    assert_eq!(ser(&ws), SRC);
    assert!(
        ws.run(
            "m",
            &Cmd::MoveBlocks {
                ids: vec![a],
                target: Target::LastChild(a1)
            }
        )
        .is_err()
    );
}

#[test]
fn duplicate_uuid_is_an_invariant_violation_and_rolls_back() {
    let u = "11111111-2222-3333-4444-555555555555";
    let mut ws = open(&format!("- a\n  id:: {u}\n- b\n"));
    let b = id(&ws, "b");
    let err = ws
        .run(
            "dup",
            &Cmd::SetProperty {
                id: b,
                key: "id".into(),
                value: u.into(),
            },
        )
        .expect_err("duplicate");
    assert!(
        matches!(
            err,
            CommitError::Invariant(InvariantError::DuplicateUuid(_))
        ),
        "{err:?}"
    );
    assert_eq!(ser(&ws), format!("- a\n  id:: {u}\n- b\n"));
}

#[test]
fn preexisting_duplicate_uuids_do_not_block_other_edits() {
    let u = "11111111-2222-3333-4444-555555555555";
    let src = format!("- a\n  id:: {u}\n- b\n  id:: {u}\n- c\n");
    let mut ws = open(&src);
    let c = id(&ws, "c");
    ws.run(
        "e",
        &Cmd::SetText {
            id: c,
            text: "c2".into(),
        },
    )
    .expect("edit");
}

#[test]
fn page_ops_create_rename_delete_and_inverse() {
    let mut ws = open(SRC);
    let new_key = PageKey::from_title("n");
    let path = GraphPath::new("pages/n.md").ok();
    let tx = ws
        .commit(
            "Create",
            vec![
                Op::CreatePage {
                    page: new_key.clone(),
                    title: "n".into(),
                    path: path.clone(),
                    restored: None,
                },
                Op::InsertSubtree {
                    page: new_key.clone(),
                    parent: None,
                    index: 0,
                    subtree: Subtree::new(ws.alloc_id(), "hello"),
                },
            ],
        )
        .expect("create");
    assert_eq!(ws.page(&new_key).expect("n").serialize(), b"- hello\n");
    assert!(ws.dirty_pages().contains(&new_key));
    ws.undo(&tx).expect("undo create");
    assert!(ws.page(&new_key).is_none());

    // Rename the file.
    let to = GraphPath::new("pages/q.md").ok();
    let tx = ws
        .commit(
            "Rename",
            vec![Op::RenameFile {
                page: key(),
                from: GraphPath::new("pages/p.md").ok(),
                to: to.clone(),
            }],
        )
        .expect("rename");
    assert_eq!(ws.page(&key()).expect("p").path, to);
    ws.undo(&tx).expect("undo rename");

    // Delete and restore, ids included.
    let a = id(&ws, "a");
    let tx = ws
        .commit(
            "Delete page",
            vec![Op::DeletePage {
                page: key(),
                captured: None,
            }],
        )
        .expect("delete");
    assert!(ws.page(&key()).is_none() && ws.locate(a).is_none());
    assert!(
        ws.pending_deletes()
            .contains_key(&GraphPath::new("pages/p.md").expect("p"))
    );
    ws.undo(&tx).expect("restore");
    assert_eq!(ser(&ws), SRC);
    assert_eq!(id(&ws, "a"), a);
    assert!(ws.pending_deletes().is_empty());
}

#[test]
fn cross_page_move_and_back_restores_bytes() {
    let mut ws = open(SRC);
    let other = PageKey::from_title("o");
    ws.load_page(
        other.clone(),
        "o",
        GraphPath::new("pages/o.md").ok(),
        b"- x\n",
    );
    let o_ser = |ws: &Workspace| ws.page(&other).expect("o").serialize();
    let b = id(&ws, "b");
    let tx = ws
        .commit(
            "Move across",
            vec![Op::Move {
                id: b,
                from: None,
                to: Position {
                    page: other.clone(),
                    parent: None,
                    index: 1,
                },
            }],
        )
        .expect("move");
    assert_eq!(o_ser(&ws), b"- x\n- b\n\t- b1\n");
    assert_eq!(ws.locate(b), Some(&other));
    ws.undo(&tx).expect("undo");
    assert_eq!(ser(&ws), SRC);
    assert_eq!(o_ser(&ws), b"- x\n");
}

#[test]
fn adopt_children_and_inverse() {
    let mut ws = open(SRC);
    let (a, b) = (id(&ws, "a"), id(&ws, "b"));
    let tx = ws
        .commit(
            "Adopt",
            vec![Op::AdoptChildren {
                from: a,
                to: b,
                at: 1,
                moved: vec![],
                src_at: None,
            }],
        )
        .expect("adopt");
    assert_eq!(
        ser(&ws),
        "title:: T\n\n- a\n- b\n\t- b1\n\t- a1\n\t- a2\n- c\n"
    );
    ws.undo(&tx).expect("undo");
    assert_eq!(ser(&ws), SRC);
}

#[test]
fn refusals_explain_noops() {
    let ws = open(SRC);
    let (a, a1) = (id(&ws, "a"), id(&ws, "a1"));
    assert!(matches!(
        bitacora_core::editor::plan(&ws, &Cmd::Indent { ids: vec![a] }),
        Err(bitacora_core::editor::Refusal::NoPreviousSibling)
    ));
    assert!(matches!(
        bitacora_core::editor::plan(&ws, &Cmd::Outdent { ids: vec![a] }),
        Err(bitacora_core::editor::Refusal::AlreadyTopLevel)
    ));
    assert!(matches!(
        bitacora_core::editor::plan(
            &ws,
            &Cmd::SetText {
                id: a1,
                text: "a1".into()
            }
        ),
        Err(bitacora_core::editor::Refusal::NoChange)
    ));
}

#[test]
fn flush_writes_atomically_and_detects_external_change() {
    use bitacora_core::editor::{FileStore, FsStore};
    let dir = tempfile::tempdir().expect("tmp");
    let path = GraphPath::new("pages/p.md").expect("p");
    let mut store = FsStore::new(dir.path());
    store.write(&path, SRC.as_bytes()).expect("seed");
    let mut ws = open(SRC);
    let a = id(&ws, "a");
    ws.run(
        "e",
        &Cmd::SetText {
            id: a,
            text: "A".into(),
        },
    )
    .expect("edit");
    let r = ws.flush(&mut store);
    assert!(r.is_complete() && r.written.len() == 1, "{r:?}");
    let on_disk = std::fs::read_to_string(dir.path().join("pages/p.md")).expect("read");
    assert!(on_disk.contains("- A\n\t- a1"));
    assert!(ws.dirty_pages().is_empty());
    assert!(!dir.path().join("pages/.p.md.bitacora-tmp").exists());
    // origins were rebased: blocks are clean again and a second flush is a no-op
    let p = ws.page(&key()).expect("p");
    assert!(p.dfs().iter().all(|b| p.is_clean(*b)));

    // External edit + local edit: the external bytes are never overwritten blindly; the pre-write
    // check merges them block by block (BIT-US-0069), so both survive and no markers are written.
    store.write(&path, b"- external\n").expect("ext");
    let b = id(&ws, "b");
    ws.run(
        "e",
        &Cmd::SetText {
            id: b,
            text: "B".into(),
        },
    )
    .expect("edit");
    let r = ws.flush(&mut store);
    assert!(r.is_complete(), "{r:?}");
    let merged = std::fs::read_to_string(dir.path().join("pages/p.md")).expect("read");
    assert!(
        merged.contains("- external\n") && merged.contains("- B\n"),
        "{merged}"
    );
    assert!(!merged.contains("<<<<<<<"));
    assert!(ws.dirty_pages().is_empty());
}
