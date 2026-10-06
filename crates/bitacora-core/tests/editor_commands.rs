//! Semantic editing commands (BIT-US-0032..0038): split / merge / outdent, move, collapse,
//! markers, clipboard and autocomplete helpers. Expectations are written from the documented
//! Logseq behaviour (ADR-015); every command also checks that undo restores the exact bytes.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bitacora_core::editor::{
    BlockId, ClipBlock, Cmd, CommitError, EditorSettings, EnterAction, PasteKind, Refusal,
    Workflow, Workspace, classify_paste, enter_action, export_blocks, parse_outline, parse_private,
};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;

const U1: &str = "11111111-2222-4333-8444-555555555555";
const U2: &str = "66666666-7777-4888-8999-000000000000";

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

fn id(ws: &Workspace, text: &str) -> BlockId {
    ws.page(&key())
        .expect("page")
        .blocks
        .values()
        .find(|b| b.text == text)
        .unwrap_or_else(|| panic!("no block {text:?}"))
        .id
}

/// Runs `cmd`, checks that undo restores the bytes and redo reproduces the result, and returns
/// the result leaving the workspace in its original state.
fn run_undoable(ws: &mut Workspace, cmd: &Cmd) -> String {
    let before = ser(ws);
    let tx = ws.run("t", cmd).unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
    let after = ser(ws);
    let undo = ws.undo(&tx).expect("undo");
    assert_eq!(ser(ws), before, "undo of {cmd:?}");
    let redo = ws.undo(&undo).expect("redo");
    assert_eq!(ser(ws), after, "redo of {cmd:?}");
    ws.undo(&redo).expect("undo again");
    assert_eq!(ser(ws), before);
    after
}

fn refusal(ws: &mut Workspace, cmd: &Cmd) -> Refusal {
    let before = ser(ws);
    match ws.run("t", cmd) {
        Err(CommitError::Refused(r)) => {
            assert_eq!(ser(ws), before, "a refusal changes nothing");
            r
        }
        other => panic!("expected refusal for {cmd:?}, got {other:?}"),
    }
}

// ---- BIT-US-0032: Enter / Shift+Enter ----------------------------------------------------

#[test]
fn split_in_the_middle_left_trims_the_tail() {
    let mut ws = open("- hello world\n- z\n");
    let h = id(&ws, "hello world");
    let out = run_undoable(
        &mut ws,
        &Cmd::SplitBlock {
            id: h,
            cursor: 5..5,
        },
    );
    assert_eq!(out, "- hello\n- world\n- z\n");
}

#[test]
fn split_makes_first_child_when_children_are_shown() {
    let mut ws = open("- a b\n\t- kid\n- z\n");
    let a = id(&ws, "a b");
    let out = run_undoable(
        &mut ws,
        &Cmd::SplitBlock {
            id: a,
            cursor: 1..1,
        },
    );
    assert_eq!(out, "- a\n\t- b\n\t- kid\n- z\n");
}

#[test]
fn split_of_a_collapsed_parent_is_a_sibling() {
    let mut ws = open("- a b\n  collapsed:: true\n\t- kid\n");
    let a = id(&ws, "a b\ncollapsed:: true");
    let out = run_undoable(
        &mut ws,
        &Cmd::SplitBlock {
            id: a,
            cursor: 1..1,
        },
    );
    assert!(
        out.starts_with("- a\n  collapsed:: true\n\t- kid\n"),
        "{out}"
    );
    assert!(out.contains("- b\n") || out.contains("\n- b"), "{out}");
}

#[test]
fn caret_at_zero_inserts_an_empty_block_before_and_keeps_the_caret() {
    let mut ws = open("- one\n- two\n");
    let two = id(&ws, "two");
    let tx = ws
        .run(
            "t",
            &Cmd::SplitBlock {
                id: two,
                cursor: 0..0,
            },
        )
        .expect("split");
    assert_eq!(ser(&ws), "- one\n-\n- two\n");
    let cur = tx.cursor_after.clone().expect("cursor");
    assert_eq!((cur.block, cur.selection), (two, 0..0));
    ws.undo(&tx).expect("undo");
    assert_eq!(ser(&ws), "- one\n- two\n");
}

#[test]
fn split_keeps_identity_and_hidden_properties_on_the_original() {
    let src = format!("- title\n  id:: {U1}\n- z\n");
    let mut ws = open(&src);
    let t = id(&ws, &format!("title\nid:: {U1}"));
    let tx = ws
        .run(
            "t",
            &Cmd::SplitBlock {
                id: t,
                cursor: 2..2,
            },
        )
        .expect("split");
    // The left block keeps `id::`; the right block starts with the text after the caret.
    assert_eq!(ser(&ws), format!("- ti\n  id:: {U1}\n- tle\n- z\n"));
    assert_eq!(
        ws.block(t).expect("b").uuid.map(|u| u.to_string()),
        Some(U1.to_owned())
    );
    let cur = tx.cursor_after.clone().expect("cursor");
    assert_ne!(cur.block, t);
    ws.undo(&tx).expect("undo");
    assert_eq!(ser(&ws), src);
}

#[test]
fn split_with_selection_deletes_it() {
    let mut ws = open("- abcdef\n");
    let b = id(&ws, "abcdef");
    let out = run_undoable(
        &mut ws,
        &Cmd::SplitBlock {
            id: b,
            cursor: 2..4,
        },
    );
    assert_eq!(out, "- ab\n- ef\n");
}

#[test]
fn bad_caret_is_refused() {
    let mut ws = open("- äb\n");
    let b = id(&ws, "äb");
    assert_eq!(
        refusal(
            &mut ws,
            &Cmd::SplitBlock {
                id: b,
                cursor: 1..1
            }
        ),
        Refusal::BadCursor
    );
}

#[test]
fn enter_on_empty_last_child_outdents() {
    let mut ws = open("- a\n\t- x\n\t- \n- z\n");
    let empty = id(&ws, "");
    let out = run_undoable(
        &mut ws,
        &Cmd::Enter {
            id: empty,
            cursor: 0..0,
            zoom_root: None,
        },
    );
    assert_eq!(out, "- a\n\t- x\n-\n- z\n");
}

#[test]
fn enter_on_empty_middle_child_splits_and_zoom_root_blocks_outdent() {
    let mut ws = open("- a\n\t- \n\t- y\n");
    let empty = id(&ws, "");
    let a = id(&ws, "a");
    let act = enter_action(&ws, empty, &(0..0), None).expect("action");
    assert_eq!(act, EnterAction::Split);
    // Last child whose parent is the zoomed root keeps splitting.
    let mut ws2 = open("- a\n\t- x\n\t- \n");
    let e2 = id(&ws2, "");
    let a2 = id(&ws2, "a");
    assert_eq!(
        enter_action(&ws2, e2, &(0..0), None).expect("a"),
        EnterAction::Outdent
    );
    assert_eq!(
        enter_action(&ws2, e2, &(0..0), Some(a2)).expect("a"),
        EnterAction::Split
    );
    let _ = (a, &mut ws2);
    let _ = &mut ws;
}

#[test]
fn outdent_empty_last_is_refused_when_not_applicable() {
    let mut ws = open("- a\n\t- x\n");
    let x = id(&ws, "x");
    assert_eq!(
        refusal(&mut ws, &Cmd::OutdentEmptyLast { id: x }),
        Refusal::NotEmptyLastChild
    );
}

#[test]
fn shift_enter_adds_a_continuation_line() {
    let mut ws = open("- a\n\t- bc\n");
    let b = id(&ws, "bc");
    let out = run_undoable(&mut ws, &Cmd::InsertNewline { id: b, at: 1..1 });
    assert_eq!(out, "- a\n\t- b\n\t  c\n");
}

#[test]
fn enter_in_code_fence_and_page_ref_does_not_split() {
    let ws = open("- ```\n  code\n  ```\n- [[Page]]\n");
    let code = id(&ws, "```\ncode\n```");
    let at = "```\nco".len();
    assert_eq!(
        enter_action(&ws, code, &(at..at), None).expect("a"),
        EnterAction::Newline
    );
    let r = id(&ws, "[[Page]]");
    assert_eq!(
        enter_action(&ws, r, &(4..4), None).expect("a"),
        EnterAction::MoveCaret(8)
    );
}

// ---- BIT-US-0033: merge ------------------------------------------------------------------

#[test]
fn backspace_merges_into_previous_and_adopts_children() {
    let mut ws = open("- one\n- two\n\t- kid\n");
    let two = id(&ws, "two");
    let tx = ws
        .run("t", &Cmd::MergeWithPrevious { id: two })
        .expect("merge");
    assert_eq!(ser(&ws), "- onetwo\n\t- kid\n");
    let cur = tx.cursor_after.clone().expect("cursor");
    assert_eq!(cur.selection, 3..3, "caret at the junction");
    ws.undo(&tx).expect("undo");
    assert_eq!(ser(&ws), "- one\n- two\n\t- kid\n");
}

#[test]
fn merge_into_previous_visible_block_and_into_parent() {
    let mut ws = open("- a\n\t- a1\n- b\n");
    let b = id(&ws, "b");
    let out = run_undoable(&mut ws, &Cmd::MergeWithPrevious { id: b });
    assert_eq!(out, "- a\n\t- a1b\n");
    let mut ws = open("- p\n\t- c\n");
    let c = id(&ws, "c");
    let out = run_undoable(&mut ws, &Cmd::MergeWithPrevious { id: c });
    assert_eq!(out, "- pc\n");
}

#[test]
fn merge_refused_when_both_have_children_or_first_block() {
    let mut ws = open("- a\n\t- a1\n- b\n\t- b1\n");
    let b = id(&ws, "b");
    assert_eq!(
        refusal(&mut ws, &Cmd::MergeWithPrevious { id: b }),
        Refusal::BothHaveChildren
    );
    let a = id(&ws, "a");
    assert_eq!(
        refusal(&mut ws, &Cmd::MergeWithPrevious { id: a }),
        Refusal::FirstBlock
    );
}

#[test]
fn empty_first_block_is_deleted() {
    let mut ws = open("- \n- b\n");
    let e = id(&ws, "");
    let out = run_undoable(&mut ws, &Cmd::MergeWithPrevious { id: e });
    assert_eq!(out, "- b\n");
    let mut ws = open("- \n");
    let e = id(&ws, "");
    assert_eq!(
        refusal(&mut ws, &Cmd::MergeWithPrevious { id: e }),
        Refusal::LastBlockOfPage
    );
}

#[test]
fn referenced_block_identity_survives_a_backspace_merge() {
    let src = format!("- one\n- two\n  id:: {U1}\n  tag:: x\n- ref (({U1}))\n");
    let mut ws = open(&src);
    let two = id(&ws, &format!("two\nid:: {U1}\ntag:: x"));
    let tx = ws
        .run("t", &Cmd::MergeWithPrevious { id: two })
        .expect("merge");
    // The survivor takes over the id (and properties); the reference still resolves.
    let out = ser(&ws);
    assert!(
        out.starts_with(&format!("- onetwo\n  id:: {U1}\n  tag:: x\n")),
        "{out}"
    );
    let one = id(&ws, &format!("onetwo\nid:: {U1}\ntag:: x"));
    assert_eq!(
        ws.block(one).expect("b").uuid.map(|u| u.to_string()),
        Some(U1.to_owned())
    );
    ws.undo(&tx).expect("undo");
    assert_eq!(ser(&ws), src, "undo restores the id:: lines byte-exactly");
}

#[test]
fn merge_refused_when_both_blocks_have_ids() {
    let src = format!("- one\n  id:: {U1}\n- two\n  id:: {U2}\n");
    let mut ws = open(&src);
    let two = id(&ws, &format!("two\nid:: {U2}"));
    assert_eq!(
        refusal(&mut ws, &Cmd::MergeWithPrevious { id: two }),
        Refusal::BothReferenced
    );
    // The previous block keeps its own id when only the previous one has one.
    let src = format!("- one\n  id:: {U1}\n- two\n");
    let mut ws = open(&src);
    let two = id(&ws, "two");
    let out = run_undoable(&mut ws, &Cmd::MergeWithPrevious { id: two });
    assert_eq!(out, format!("- onetwo\n  id:: {U1}\n"));
}

#[test]
fn delete_pulls_in_next_sibling_or_first_child() {
    let mut ws = open("- ab\n- cd\n- ef\n");
    let ab = id(&ws, "ab");
    let tx = ws.run("t", &Cmd::MergeNext { id: ab }).expect("merge");
    assert_eq!(ser(&ws), "- abcd\n- ef\n");
    assert_eq!(tx.cursor_after.expect("c").selection, 2..2);
    let mut ws = open("- ab\n\t- cd\n\t- ef\n");
    let ab = id(&ws, "ab");
    let out = run_undoable(&mut ws, &Cmd::MergeNext { id: ab });
    assert_eq!(out, "- abcd\n\t- ef\n");
}

#[test]
fn delete_refusals_and_children_of_next_come_along() {
    let mut ws = open("- ab\n\t- cd\n\t\t- deep\n");
    let ab = id(&ws, "ab");
    assert_eq!(
        refusal(&mut ws, &Cmd::MergeNext { id: ab }),
        Refusal::BothHaveChildren
    );
    let mut ws = open("- ab\n- cd\n\t- kid\n");
    let ab = id(&ws, "ab");
    let out = run_undoable(&mut ws, &Cmd::MergeNext { id: ab });
    assert_eq!(out, "- abcd\n\t- kid\n");
    let mut ws = open("- ab\n");
    let ab = id(&ws, "ab");
    assert_eq!(
        refusal(&mut ws, &Cmd::MergeNext { id: ab }),
        Refusal::NoNextBlock
    );
}

// ---- BIT-US-0034: indent / outdent / move -------------------------------------------------

#[test]
fn logical_outdenting_keeps_following_siblings() {
    let src = "- p\n\t- a\n\t- b\n\t- c\n";
    let mut ws = open(src);
    let a = id(&ws, "a");
    let direct = run_undoable(&mut ws, &Cmd::Outdent { ids: vec![a] });
    assert_eq!(direct, "- p\n- a\n\t- b\n\t- c\n");
    ws.set_settings(EditorSettings {
        logical_outdenting: true,
        workflow: Workflow::Now,
    });
    let logical = run_undoable(&mut ws, &Cmd::Outdent { ids: vec![a] });
    assert_eq!(logical, "- p\n\t- b\n\t- c\n- a\n");
}

#[test]
fn settings_come_from_config_edn() {
    let cfg = bitacora_config::EffectiveConfig::from_texts(
        None,
        Some("{:editor/logical-outdenting? true :preferred-workflow :todo}"),
    );
    let s = EditorSettings::from_config(&cfg);
    assert!(s.logical_outdenting);
    assert_eq!(s.workflow, Workflow::Todo);
}

#[test]
fn move_up_down_swaps_and_crosses_parents() {
    let src = "- a\n\t- a1\n- b\n\t- b1\n\t- b2\n- c\n";
    let mut ws = open(src);
    let (b, b1, c, a1) = (id(&ws, "b"), id(&ws, "b1"), id(&ws, "c"), id(&ws, "a1"));
    assert_eq!(
        run_undoable(
            &mut ws,
            &Cmd::MoveUpDown {
                ids: vec![b],
                up: true
            }
        ),
        "- b\n\t- b1\n\t- b2\n- a\n\t- a1\n- c\n"
    );
    // First child moving up goes to the end of the previous uncle's children.
    assert_eq!(
        run_undoable(
            &mut ws,
            &Cmd::MoveUpDown {
                ids: vec![b1],
                up: true
            }
        ),
        "- a\n\t- a1\n\t- b1\n- b\n\t- b2\n- c\n"
    );
    // Last child moving down becomes the first child of the next uncle.
    assert_eq!(
        run_undoable(
            &mut ws,
            &Cmd::MoveUpDown {
                ids: vec![a1],
                up: false
            }
        ),
        "- a\n- b\n\t- a1\n\t- b1\n\t- b2\n- c\n"
    );
    // Page edges are refused.
    assert_eq!(
        refusal(
            &mut ws,
            &Cmd::MoveUpDown {
                ids: vec![c],
                up: false
            }
        ),
        Refusal::AtEdge
    );
}

#[test]
fn move_up_down_works_on_selections_and_rejects_gaps() {
    let mut ws = open("- a\n- b\n- c\n- d\n");
    let (a, b, c, d) = (id(&ws, "a"), id(&ws, "b"), id(&ws, "c"), id(&ws, "d"));
    assert_eq!(
        run_undoable(
            &mut ws,
            &Cmd::MoveUpDown {
                ids: vec![b, c],
                up: false
            }
        ),
        "- a\n- d\n- b\n- c\n"
    );
    assert_eq!(
        refusal(
            &mut ws,
            &Cmd::MoveUpDown {
                ids: vec![a, c],
                up: true
            }
        ),
        Refusal::NotSiblings
    );
    let _ = d;
}

#[test]
fn moved_clean_blocks_keep_their_bytes() {
    let src = "- a  \n- b *x*\n- c\n";
    let mut ws = open(src);
    let b = id(&ws, "b *x*");
    let out = run_undoable(
        &mut ws,
        &Cmd::MoveUpDown {
            ids: vec![b],
            up: true,
        },
    );
    assert_eq!(out, "- b *x*\n- a  \n- c\n");
}

// ---- BIT-US-0035/0036: collapse, markers, bulk ops ----------------------------------------

#[test]
fn collapse_ignores_leaves_and_expand_restores_bytes() {
    let src = "- p\n\t- k\n- leaf\n";
    let mut ws = open(src);
    let (p, leaf) = (id(&ws, "p"), id(&ws, "leaf"));
    assert_eq!(
        refusal(
            &mut ws,
            &Cmd::CollapseBlocks {
                ids: vec![leaf],
                collapsed: true
            }
        ),
        Refusal::NoChange
    );
    let tx = ws
        .run(
            "t",
            &Cmd::CollapseBlocks {
                ids: vec![p, leaf],
                collapsed: true,
            },
        )
        .expect("collapse");
    assert_eq!(ser(&ws), "- p\n  collapsed:: true\n\t- k\n- leaf\n");
    let _ = tx;
    let p = id(&ws, "p\ncollapsed:: true");
    ws.run(
        "t",
        &Cmd::CollapseBlocks {
            ids: vec![p],
            collapsed: false,
        },
    )
    .expect("expand");
    assert_eq!(ser(&ws), src);
}

#[test]
fn page_level_collapse_goes_one_level_at_a_time() {
    let src = "- a\n\t- b\n\t\t- c\n- d\n\t- e\n";
    let mut ws = open(src);
    ws.run(
        "t",
        &Cmd::CollapseLevel {
            page: key(),
            collapse: true,
        },
    )
    .expect("level 1");
    // Deepest expanded parents (b) fold first.
    assert!(
        ser(&ws).contains("\t- b\n\t  collapsed:: true\n"),
        "{}",
        ser(&ws)
    );
    assert!(!ser(&ws).contains("- a\n  collapsed"));
    ws.run(
        "t",
        &Cmd::CollapseLevel {
            page: key(),
            collapse: true,
        },
    )
    .expect("level 2");
    ws.run(
        "t",
        &Cmd::SetAllCollapsed {
            page: key(),
            collapsed: false,
        },
    )
    .expect("expand all");
    assert_eq!(ser(&ws), src);
}

#[test]
fn cycle_marker_follows_the_preferred_workflow() {
    let mut ws = open("- task\n- x\n");
    let t = id(&ws, "task");
    let steps_now = ["LATER task", "NOW task", "DONE task", "task"];
    for want in steps_now {
        let cur = ws
            .page(&key())
            .expect("p")
            .block(ws.page(&key()).expect("p").roots[0])
            .expect("b")
            .id;
        ws.run("t", &Cmd::CycleMarker { ids: vec![cur] })
            .expect("cycle");
        assert_eq!(ser(&ws), format!("- {want}\n- x\n"));
    }
    ws.set_settings(EditorSettings {
        logical_outdenting: false,
        workflow: Workflow::Todo,
    });
    let _ = t;
    let first = ws.page(&key()).expect("p").roots[0];
    for want in ["TODO task", "DOING task", "DONE task", "task"] {
        ws.run("t", &Cmd::CycleMarker { ids: vec![first] })
            .expect("cycle");
        assert_eq!(ser(&ws), format!("- {want}\n- x\n"));
    }
}

#[test]
fn markers_keep_priority_and_properties_and_skip_empty_blocks() {
    let src = "- [#A] ship it\n  owner:: me\n- \n";
    let mut ws = open(src);
    let (a, e) = (id(&ws, "[#A] ship it\nowner:: me"), id(&ws, ""));
    let out = run_undoable(&mut ws, &Cmd::CycleMarker { ids: vec![a, e] });
    assert_eq!(out, "- LATER [#A] ship it\n  owner:: me\n- \n");
    let out = run_undoable(&mut ws, &Cmd::ToggleDone { ids: vec![a] });
    assert!(out.starts_with("- DONE [#A] ship it"), "{out}");
    assert_eq!(
        refusal(
            &mut ws,
            &Cmd::SetMarker {
                ids: vec![a],
                marker: Some("bogus".into())
            }
        ),
        Refusal::NothingApplicable
    );
}

#[test]
fn toggle_done_unchecks_to_the_workflow_start() {
    let mut ws = open("- DONE x\n");
    let x = id(&ws, "DONE x");
    assert_eq!(
        run_undoable(&mut ws, &Cmd::ToggleDone { ids: vec![x] }),
        "- LATER x\n"
    );
}

#[test]
fn bulk_operations_on_a_selection_are_one_transaction() {
    let mut ws = open("- a\n- b\n- c\n- d\n");
    let (b, c) = (id(&ws, "b"), id(&ws, "c"));
    let tx = ws
        .run("t", &Cmd::Indent { ids: vec![b, c] })
        .expect("indent");
    assert_eq!(ser(&ws), "- a\n\t- b\n\t- c\n- d\n");
    ws.undo(&tx).expect("undo");
    assert_eq!(ser(&ws), "- a\n- b\n- c\n- d\n");
    assert_eq!(
        run_undoable(&mut ws, &Cmd::DeleteBlocks { ids: vec![b, c] }),
        "- a\n- d\n"
    );
    let a = id(&ws, "a");
    let d = id(&ws, "d");
    assert_eq!(
        run_undoable(
            &mut ws,
            &Cmd::SetMarker {
                ids: vec![a, d],
                marker: Some("TODO".into())
            }
        ),
        "- TODO a\n- b\n- c\n- TODO d\n"
    );
}

// ---- BIT-US-0037: clipboard --------------------------------------------------------------

const TREE: &str = "- a\n\t- a1\n\t  more\n- b\n  id:: 11111111-2222-4333-8444-555555555555\n";

#[test]
fn copy_exports_tab_indented_markdown_without_ids() {
    let ws = open(TREE);
    let ids: Vec<BlockId> = ws.page(&key()).expect("p").dfs();
    let p = export_blocks(&ws, &ids, false).expect("export");
    assert_eq!(p.text, "- a\n\t- a1\n\t  more\n- b\n");
    assert!(
        p.html
            .starts_with("<ul><li>a<ul><li>a1<br>more</li></ul></li>")
    );
    assert!(p.private.contains(U1), "private payload keeps the id");
    let (cut, blocks) = parse_private(&p.private).expect("private");
    assert!(!cut);
    assert_eq!(blocks[0].children[0].text, "a1\nmore");
}

#[test]
fn selection_in_any_order_exports_top_level_blocks_in_page_order() {
    let ws = open(TREE);
    let (a1, a, b) = (
        id(&ws, "a1\nmore"),
        id(&ws, "a"),
        id(&ws, &format!("b\nid:: {U1}")),
    );
    let p = export_blocks(&ws, &[b, a1, a], false).expect("export");
    assert_eq!(p.text, "- a\n\t- a1\n\t  more\n- b\n");
}

#[test]
fn cut_deletes_in_one_transaction_and_paste_keeps_ids() {
    let mut ws = open(TREE);
    let b = id(&ws, &format!("b\nid:: {U1}"));
    let (payload, tx) = ws.cut_blocks("Cut", &[b]).expect("cut");
    assert_eq!(ser(&ws), "- a\n\t- a1\n\t  more\n");
    let a1 = id(&ws, "a1\nmore");
    let pasted = ws
        .paste_private("Paste", a1, &payload.private)
        .expect("paste")
        .expect("private");
    assert!(
        ser(&ws).contains(&format!("- b\n\t  id:: {U1}")),
        "{}",
        ser(&ws)
    );
    ws.undo(&pasted).expect("undo paste");
    ws.undo(&tx).expect("undo cut");
    assert_eq!(ser(&ws), TREE);
}

#[test]
fn paste_after_copy_gets_fresh_identity() {
    let mut ws = open(TREE);
    let b = id(&ws, &format!("b\nid:: {U1}"));
    let payload = export_blocks(&ws, &[b], false).expect("copy");
    ws.paste_private("Paste", b, &payload.private)
        .expect("paste")
        .expect("tx");
    let out = ser(&ws);
    assert_eq!(out.matches(U1).count(), 1, "the copy has no id::\n{out}");
    assert!(out.ends_with("- b\n"), "{out}");
}

#[test]
fn text_paste_classification() {
    assert!(matches!(classify_paste("a\nb", false), PasteKind::Inline(t) if t == "a\nb"));
    assert!(
        matches!(classify_paste("- x\r\n\t- y", false), PasteKind::Blocks(b) if b.len() == 1 && b[0].children.len() == 1)
    );
    assert!(matches!(
        classify_paste("# Title\ntext", false),
        PasteKind::Blocks(_)
    ));
    match classify_paste("para one\nline 2\n\npara two", false) {
        PasteKind::Blocks(b) => {
            assert_eq!(b.len(), 2);
            assert_eq!(b[0].text, "para one\nline 2");
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        classify_paste("- x\n- y", true),
        PasteKind::Inline(_)
    ));
    assert!(matches!(
        classify_paste("2 * 3 = 6", false),
        PasteKind::Inline(_)
    ));
}

#[test]
fn pasting_a_markdown_outline_builds_a_tree_and_replaces_an_empty_target() {
    let mut ws = open("- a\n- \n- z\n");
    let e = id(&ws, "");
    let out = run_undoable(
        &mut ws,
        &Cmd::PasteText {
            target: e,
            cursor: 0..0,
            text: "- x\r\n\t- y\r\n- w".into(),
            raw: false,
        },
    );
    assert_eq!(out, "- a\n- x\n\t- y\n- w\n- z\n");
}

#[test]
fn pasting_into_a_block_with_text_goes_after_it_and_inline_goes_at_the_caret() {
    let mut ws = open("- a\n- b\n");
    let a = id(&ws, "a");
    assert_eq!(
        run_undoable(
            &mut ws,
            &Cmd::PasteText {
                target: a,
                cursor: 1..1,
                text: "- n1\n- n2".into(),
                raw: false
            }
        ),
        "- a\n- n1\n- n2\n- b\n"
    );
    assert_eq!(
        run_undoable(
            &mut ws,
            &Cmd::PasteText {
                target: a,
                cursor: 0..1,
                text: "xyz".into(),
                raw: false
            }
        ),
        "- xyz\n- b\n"
    );
}

#[test]
fn insert_blocks_dedupes_ids_that_exist() {
    let src = format!("- a\n  id:: {U1}\n");
    let mut ws = open(&src);
    let a = id(&ws, &format!("a\nid:: {U1}"));
    let blocks = vec![
        ClipBlock::leaf(format!("dup\nid:: {U1}")),
        ClipBlock::leaf(format!("new\nid:: {U2}")),
    ];
    let out = run_undoable(
        &mut ws,
        &Cmd::InsertBlocks {
            target: a,
            sibling: None,
            blocks,
            keep_uuids: true,
        },
    );
    assert_eq!(out.matches(U1).count(), 1, "{out}");
    assert_eq!(out.matches(U2).count(), 1, "{out}");
}

#[test]
fn parse_outline_nests_by_depth() {
    let t = parse_outline("intro\n- a\n  - b\n    - c\n- d");
    assert_eq!(t.len(), 3);
    assert_eq!(t[0].text, "intro");
    assert_eq!(t[1].children[0].children[0].text, "c");
}
