//! Undo/redo history (BIT-US-0039; BIT-SP-0004.R5, R17, R18) and autocomplete helpers
//! (BIT-US-0038): coalescing, cursor restore, caps, byte-exact files through the writer, and a
//! property test over random sequences of the semantic commands.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::{Duration, Instant};

use bitacora_core::editor::{
    BlockId, Cmd, CommitError, CursorState, History, HistoryConfig, HistoryError, MemStore,
    Refusal, Target, Transaction, Workspace, WorkspaceProvider, block_candidates, block_ref_text,
    complete_page, page_candidates, trigger_range,
};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;
use bitacora_core::queue::{CommandQueue, QueueConfig, Source};
use proptest::prelude::*;

fn key() -> PageKey {
    PageKey::from_title("p")
}

fn path() -> GraphPath {
    GraphPath::new("pages/p.md").expect("path")
}

fn open(src: &[u8]) -> Workspace {
    let mut ws = Workspace::new();
    ws.load_page(key(), "p", Some(path()), src);
    ws
}

fn ser(ws: &Workspace) -> Vec<u8> {
    ws.page(&key()).expect("page").serialize()
}

fn text_of(ws: &Workspace, id: BlockId) -> String {
    ws.block(id).expect("block").text.clone()
}

fn first(ws: &Workspace) -> BlockId {
    ws.page(&key()).expect("p").roots[0]
}

/// A typing transaction at a given time offset.
fn typed(ws: &mut Workspace, id: BlockId, at: usize, s: &str, t0: Instant, ms: u64) -> Transaction {
    let mut tx = ws
        .run(
            "Typing",
            &Cmd::EditText {
                id,
                range: at..at,
                inserted: s.to_owned(),
            },
        )
        .expect("type");
    tx.at = t0 + Duration::from_millis(ms);
    tx
}

#[test]
fn typing_coalesces_within_the_gap_and_splits_after_it() {
    let mut ws = open(b"- \n- z\n");
    let a = first(&ws);
    let mut h = History::new();
    let t0 = Instant::now();
    for (i, (ch, ms)) in [("a", 0), ("b", 100), ("c", 300), ("d", 900)]
        .into_iter()
        .enumerate()
    {
        let tx = typed(&mut ws, a, i, ch, t0, ms);
        h.push(tx);
    }
    assert_eq!(h.undo_len(), 1, "one typing run");
    assert_eq!(text_of(&ws, a), "abcd");
    // A pause longer than 1.5 s starts a new step.
    let tx = typed(&mut ws, a, 4, "e", t0, 2_600);
    h.push(tx);
    assert_eq!(h.undo_len(), 2);
    let step = h.undo(&mut ws).expect("undo");
    assert_eq!(text_of(&ws, a), "abcd");
    assert!(step.cursor.is_some());
    h.undo(&mut ws).expect("undo");
    assert_eq!(text_of(&ws, a), "");
    assert!(matches!(h.undo(&mut ws), Err(HistoryError::NothingToUndo)));
    h.redo(&mut ws).expect("redo");
    assert_eq!(text_of(&ws, a), "abcd");
}

#[test]
fn word_boundary_after_a_pause_and_structural_ops_break_the_run() {
    let mut ws = open(b"- \n- z\n");
    let a = first(&ws);
    let mut h = History::new();
    let t0 = Instant::now();
    h.push(typed(&mut ws, a, 0, "hi", t0, 0));
    // A space after a 600 ms pause starts a new step; fast typing of the same space would not.
    h.push(typed(&mut ws, a, 2, " ", t0, 600));
    assert_eq!(h.undo_len(), 2);
    h.push(typed(&mut ws, a, 3, "x", t0, 650));
    assert_eq!(h.undo_len(), 2, "continues the previous run");
    let tx = ws
        .run(
            "Indent",
            &Cmd::Indent {
                ids: vec![ws.page(&key()).expect("p").roots[1]],
            },
        )
        .expect("indent");
    h.push(tx);
    h.push(typed(&mut ws, a, 4, "y", t0, 700));
    assert_eq!(h.undo_len(), 4, "a structural op in between ends the run");
}

#[test]
fn new_transactions_clear_redo_and_undo_restores_cursors() {
    let mut ws = open(b"- hello world\n");
    let a = first(&ws);
    let mut h = History::new();
    let tx = ws
        .run(
            "Split",
            &Cmd::SplitBlock {
                id: a,
                cursor: 5..5,
            },
        )
        .expect("split");
    let after = tx.cursor_after.clone().expect("after");
    assert_eq!(
        tx.cursor_before,
        Some(CursorState {
            block: a,
            selection: 5..5
        })
    );
    h.push(tx);
    let undo = h.undo(&mut ws).expect("undo");
    assert_eq!(
        undo.cursor,
        Some(CursorState {
            block: a,
            selection: 5..5
        })
    );
    let redo = h.redo(&mut ws).expect("redo");
    assert_eq!(redo.cursor, Some(after));
    h.undo(&mut ws).expect("undo");
    assert_eq!(h.redo_len(), 1);
    let tx = ws
        .run(
            "Set",
            &Cmd::SetText {
                id: a,
                text: "other".into(),
            },
        )
        .expect("set");
    h.push(tx);
    assert_eq!(h.redo_len(), 0, "a new transaction clears redo");
    assert!(matches!(h.redo(&mut ws), Err(HistoryError::NothingToRedo)));
}

#[test]
fn history_is_capped_by_entries_and_bytes() {
    let mut ws = open(b"- a\n");
    let a = first(&ws);
    let mut h = History::with_config(HistoryConfig {
        max_entries: 3,
        ..HistoryConfig::default()
    });
    for i in 0..10 {
        let tx = ws
            .run(
                "Set",
                &Cmd::SetText {
                    id: a,
                    text: format!("v{i}"),
                },
            )
            .expect("set");
        h.push(tx);
    }
    assert_eq!(h.undo_len(), 3);
    let mut h = History::with_config(HistoryConfig {
        max_bytes: 400,
        ..HistoryConfig::default()
    });
    for i in 0..50 {
        let tx = ws
            .run(
                "Set",
                &Cmd::SetText {
                    id: a,
                    text: format!("{i:0>40}"),
                },
            )
            .expect("set");
        h.push(tx);
    }
    assert!(h.undo_len() < 50 && h.undo_len() >= 1 && h.bytes() <= 800);
}

#[test]
fn undo_stops_with_a_notice_when_an_external_change_removed_the_target() {
    let mut ws = open(b"- a\n- b\n");
    let a = first(&ws);
    let mut h = History::new();
    let tx = ws
        .run(
            "Set",
            &Cmd::SetText {
                id: a,
                text: "A".into(),
            },
        )
        .expect("set");
    h.push(tx);
    // The page is replaced by content where the block no longer exists.
    ws.load_page(key(), "p", Some(path()), b"- other\n");
    let err = h.undo(&mut ws).expect_err("truncated");
    assert!(matches!(err, HistoryError::Truncated(_)));
    assert_eq!(err.to_string(), "history truncated by external change");
    assert_eq!(h.undo_len(), 0);
    assert_eq!(ser(&ws), b"- other\n");
}

#[test]
fn refused_commands_leave_no_history_entry() {
    let mut ws = open(b"- a\n");
    let a = first(&ws);
    let h = History::new();
    assert!(matches!(
        ws.run("Merge", &Cmd::MergeWithPrevious { id: a }),
        Err(CommitError::Refused(
            Refusal::FirstBlock | Refusal::LastBlockOfPage
        ))
    ));
    assert_eq!(h.undo_len(), 0);
}

/// BIT-T-0265: after undo the file written by the writer is byte-identical to the original,
/// including CRLF, blank lines, trailing spaces and odd indentation (a file without a final
/// newline is exact as long as nothing is appended after its last line).
#[test]
fn undo_through_the_writer_restores_the_file_bytes() {
    let sources: [&[u8]; 3] = [
        b"\xef\xbb\xbftitle:: t\r\n\r\n- a b\r\n  - c\r\n\r\n- d\r\n",
        b"- a\n    - a1\n        - a11\n    - a2\n- b\n",
        b"- p\n\t- q  \n\n\n- r\n  id:: 11111111-2222-4333-8444-555555555555\n",
    ];
    for src in sources {
        let mut ws = open(src);
        let mut store = MemStore::default();
        store.files.insert(path(), src.to_vec());
        let mut h = History::new();
        let order = ws.page(&key()).expect("p").dfs();
        assert!(
            !order.is_empty(),
            "no blocks in {:?}",
            String::from_utf8_lossy(src)
        );
        let cmds = [
            Cmd::SplitBlock {
                id: order[0],
                cursor: 1..1,
            },
            Cmd::MergeNext { id: order[0] },
            Cmd::InsertNewline {
                id: order[0],
                at: 0..0,
            },
            Cmd::SetCollapsed {
                ids: vec![order[0]],
                collapsed: true,
            },
            Cmd::PasteText {
                target: order[order.len() - 1],
                cursor: 0..0,
                text: "- x\n\t- y".into(),
                raw: false,
            },
            Cmd::DeleteBlocks {
                ids: vec![order[order.len() - 1]],
            },
        ];
        let mut applied = 0;
        for c in &cmds {
            if let Ok(tx) = ws.run("step", c) {
                h.push(tx);
                applied += 1;
                let r = ws.flush(&mut store);
                assert!(r.is_complete(), "{r:?}");
            }
        }
        assert!(applied >= 2, "{applied}");
        while h.undo(&mut ws).is_ok() {
            let r = ws.flush(&mut store);
            assert!(r.is_complete(), "{r:?}");
        }
        assert_eq!(
            store.files[&path()],
            src,
            "file after undo-all: {:?}",
            String::from_utf8_lossy(&store.files[&path()])
        );
    }
}

#[test]
fn queue_undo_redo_roundtrip_and_restore_cursor() {
    let src = "- hello world\n- z\n";
    let mut ws = Workspace::new();
    ws.load_page(key(), "p", Some(path()), src.as_bytes());
    let mut store = MemStore::default();
    store.files.insert(path(), src.as_bytes().to_vec());
    let (q, join) = CommandQueue::spawn(
        ws,
        Box::new(store),
        QueueConfig {
            debounce: None,
            ..QueueConfig::default()
        },
    );
    let snap = q.snapshot(&key()).expect("snapshot");
    let a = snap.blocks[0].id;
    q.run(
        Source::Ui,
        "Split",
        Cmd::SplitBlock {
            id: a,
            cursor: 5..5,
        },
    )
    .expect("split");
    assert_eq!(q.snapshot(&key()).expect("s").blocks.len(), 3);
    let step = q.undo(Source::Ui).expect("queue").expect("undo");
    assert_eq!(step.cursor.expect("c").selection, 5..5);
    assert_eq!(q.snapshot(&key()).expect("s").blocks.len(), 2);
    q.redo(Source::Ui).expect("queue").expect("redo");
    assert_eq!(q.snapshot(&key()).expect("s").blocks.len(), 3);
    q.undo(Source::Ui).expect("q").expect("undo");
    assert!(matches!(
        q.undo(Source::Ui).expect("q"),
        Err(HistoryError::NothingToUndo)
    ));
    let flushed = q.flush(Source::Ui).expect("flush");
    assert!(flushed.is_complete());
    let ws = join.shutdown().expect("shutdown");
    assert_eq!(ser(&ws), src.as_bytes());
}

// ---- BIT-US-0038: autocomplete core ---------------------------------------------------------

#[test]
fn block_ref_choice_adds_the_id_in_the_same_transaction() {
    let mut ws = open(b"- see ((\n- target one\n- plain\n");
    let (src, target) = (first(&ws), ws.page(&key()).expect("p").roots[1]);
    let tx = ws
        .run(
            "Ref",
            &Cmd::InsertBlockRef {
                target: src,
                range: 4..6,
                referenced: target,
            },
        )
        .expect("ref");
    let uuid = ws.block(target).expect("t").uuid.expect("uuid");
    let out = String::from_utf8(ser(&ws)).expect("utf8");
    assert_eq!(
        out,
        format!(
            "- see {}\n- target one\n  id:: {}\n- plain\n",
            block_ref_text(uuid),
            uuid.hyphenated()
        )
    );
    // Unreferenced blocks never got an id.
    assert!(!out.contains("plain\n  id::"));
    ws.undo(&tx).expect("undo");
    assert_eq!(ser(&ws), b"- see ((\n- target one\n- plain\n");
    // Referencing a block that already has an id reuses it.
    let again = ws
        .run(
            "Ref",
            &Cmd::InsertBlockRef {
                target: src,
                range: 4..6,
                referenced: target,
            },
        )
        .expect("ref");
    assert_eq!(again.ops.len(), 2);
}

#[test]
fn copy_block_ref_persists_the_id_once() {
    let mut ws = open(b"- a\n");
    let a = first(&ws);
    let (text, tx) = ws.copy_block_ref("Copy ref", a, false).expect("copy");
    assert!(tx.is_some());
    let uuid = ws.block(a).expect("a").uuid.expect("uuid");
    assert_eq!(text, block_ref_text(uuid));
    let (embed, tx) = ws.copy_block_ref("Copy embed", a, true).expect("embed");
    assert!(tx.is_none(), "already has an id");
    assert_eq!(embed, format!("{{{{embed {}}}}}", block_ref_text(uuid)));
}

#[test]
fn candidates_exclude_current_page_self_and_ancestors_and_offer_new_page() {
    let mut ws = Workspace::new();
    ws.load_page(
        PageKey::from_title("alpha"),
        "Alpha",
        GraphPath::new("pages/alpha.md").ok(),
        b"- parent alpha\n\t- child alpha\n",
    );
    ws.load_page(
        PageKey::from_title("alps"),
        "Alps",
        GraphPath::new("pages/alps.md").ok(),
        b"- other alpine\n",
    );
    let provider = WorkspaceProvider(&ws);
    let pages = page_candidates(&provider, "al", "Alpha", 10);
    assert_eq!(
        pages.iter().map(|p| p.title.as_str()).collect::<Vec<_>>(),
        ["Alps", "al"]
    );
    assert!(!pages[1].exists);
    let exact = page_candidates(&provider, "Alps", "Alpha", 10);
    assert_eq!(exact.len(), 1, "no New page entry for an existing title");
    let child = ws
        .page(&PageKey::from_title("alpha"))
        .expect("p")
        .dfs()
        .pop()
        .expect("child");
    let blocks = block_candidates(&ws, &provider, "alp", child, 20);
    assert_eq!(blocks.len(), 1, "{blocks:?}");
    assert_eq!(blocks[0].page, "Alps");
    let c = complete_page(
        "x [[al",
        trigger_range("x [[al", 6, "[[", "]]").expect("r"),
        "Alps",
        false,
        true,
    );
    assert_eq!(c.text, "[[Alps]]");
}

#[test]
fn choosing_a_new_page_creates_a_virtual_page_on_demand() {
    use bitacora_config::EffectiveConfig;
    let cfg = EffectiveConfig::from_texts(None, None);
    let mut ws = Workspace::new();
    let store = MemStore::default();
    let c = complete_page("[[Fresh", 0..7, "Fresh Page", false, false);
    assert_eq!(c.create_page.as_deref(), Some("Fresh Page"));
    let opened = ws
        .open_page("Fresh Page", &cfg, None, &store)
        .expect("create");
    assert!(ws.page(opened.key()).is_some());
    // No file until the page has content.
    assert!(ws.dirty_pages().is_empty());
}

// ---- property test --------------------------------------------------------------------------

const CORPUS: &[&str] = &[
    "- a b\n\t- a1\n\t\t- a11\n\t- a2\n- b c\n- c\n\t- c1\n",
    "title:: T\n\n- a\n  id:: 11111111-2222-4333-8444-555555555555\n- b\n  collapsed:: true\n\t- b1\n",
    "- a\r\n  - a1\r\n  - a2\r\n- b\r\n",
    "* a\n    * a1\n        * a11\n    * a2\n* b\n",
    "- TODO a\n\n\n- b\n\t- b1\n\n- c",
    "- ```\n  - not a block\n  ```\n- after\n",
    "- ünï\n\t- cödé\n- x\n",
];

fn char_boundary(text: &str, mut i: usize) -> usize {
    i = i.min(text.len());
    while !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn interpret(ws: &Workspace, kind: u8, a: usize, b: usize, text: &str) -> Option<Cmd> {
    let order = ws.page(&key())?.dfs();
    let pick = |i: usize| -> Option<BlockId> { order.get(i % order.len().max(1)).copied() };
    let id = pick(a)?;
    let t = text_of(ws, id);
    let at = char_boundary(&t, b);
    Some(match kind % 16 {
        0 => Cmd::SplitBlock { id, cursor: at..at },
        1 => Cmd::Enter {
            id,
            cursor: at..at,
            zoom_root: None,
        },
        2 => Cmd::InsertNewline { id, at: at..at },
        3 => Cmd::MergeWithPrevious { id },
        4 => Cmd::MergeNext { id },
        5 => Cmd::MoveUpDown {
            ids: vec![id],
            up: b.is_multiple_of(2),
        },
        6 => Cmd::Indent { ids: vec![id] },
        7 => Cmd::Outdent { ids: vec![id] },
        8 => Cmd::CollapseBlocks {
            ids: vec![id],
            collapsed: b.is_multiple_of(2),
        },
        9 => Cmd::CycleMarker { ids: vec![id] },
        10 => Cmd::EditText {
            id,
            range: at..at,
            inserted: text.to_owned(),
        },
        11 => Cmd::PasteText {
            target: id,
            cursor: at..at,
            text: format!("- {text}\n\t- n"),
            raw: false,
        },
        12 => Cmd::EnsureUuid { id, uuid: None },
        13 => Cmd::DeleteBlocks { ids: vec![id] },
        14 => Cmd::MoveBlocks {
            ids: vec![id],
            target: Target::After(pick(b)?),
        },
        _ => Cmd::ToggleDone { ids: vec![id] },
    })
}

fn shape(ws: &Workspace) -> Vec<(usize, String)> {
    let p = ws.page(&key()).expect("page");
    p.dfs()
        .iter()
        .map(|id| (p.depth_of(*id), p.block(*id).expect("b").text.clone()))
        .collect()
}

type Steps = Vec<(u8, usize, usize, String)>;

fn steps_strategy() -> impl Strategy<Value = Steps> {
    proptest::collection::vec(
        (any::<u8>(), 0usize..64, 0usize..64, "[a-zA-Z0-9 äé]{1,8}"),
        1..16,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Within one byte generation (no write in between, i.e. inside the debounce window) every
    /// undo step restores the exact bytes, and undoing everything restores the original file.
    #[test]
    fn random_semantic_commands_then_undo_all_restore_the_original_bytes(
        src in 0..CORPUS.len(),
        steps in steps_strategy(),
    ) {
        let original = CORPUS[src].as_bytes();
        let mut ws = open(original);
        let mut store = MemStore::default();
        store.files.insert(path(), original.to_vec());
        let mut h = History::new();
        let mut states: Vec<Vec<u8>> = vec![ser(&ws)];
        for (kind, a, b, text) in &steps {
            let Some(cmd) = interpret(&ws, *kind, *a, *b, text) else { continue };
            let before = ser(&ws);
            match ws.run("step", &cmd) {
                Ok(tx) => {
                    h.push(tx);
                    states.push(ser(&ws));
                }
                Err(_) => prop_assert_eq!(ser(&ws), before),
            }
        }
        let mut last = ser(&ws);
        while h.undo(&mut ws).is_ok() {
            last = ser(&ws);
            prop_assert!(states.contains(&last), "unexpected state {:?}", String::from_utf8_lossy(&last));
        }
        prop_assert_eq!(&last[..], original);
        // Through the writer: the file ends up byte-identical to the original.
        let r = ws.flush(&mut store);
        prop_assert!(r.is_complete());
        prop_assert_eq!(&store.files[&path()][..], original);
        // Redo-all ends in the final state.
        let final_state = states.last().cloned().unwrap_or_default();
        while h.redo(&mut ws).is_ok() {}
        prop_assert_eq!(ser(&ws), final_state);
    }

    /// With a write after every step undo still brings back the same blocks (structure and text).
    #[test]
    fn undo_all_after_writes_restores_the_original_blocks(
        src in 0..CORPUS.len(),
        steps in steps_strategy(),
    ) {
        let original = CORPUS[src].as_bytes();
        let mut ws = open(original);
        let mut store = MemStore::default();
        store.files.insert(path(), original.to_vec());
        let base_shape = shape(&ws);
        let mut h = History::new();
        for (kind, a, b, text) in &steps {
            let Some(cmd) = interpret(&ws, *kind, *a, *b, text) else { continue };
            if let Ok(tx) = ws.run("step", &cmd) {
                h.push(tx);
                let r = ws.flush(&mut store);
                prop_assert!(r.is_complete());
            }
        }
        while h.undo(&mut ws).is_ok() {
            let r = ws.flush(&mut store);
            prop_assert!(r.is_complete());
        }
        prop_assert_eq!(shape(&ws), base_shape);
        let reparsed = Workspace::new();
        let mut reparsed = reparsed;
        reparsed.load_page(key(), "p", Some(path()), &store.files[&path()]);
        prop_assert_eq!(shape(&reparsed).len(), shape(&ws).len());
    }
}

#[test]
fn every_planned_op_has_a_working_inverse_for_typing() {
    // A typing run merged into one entry still inverts to the exact original.
    let mut ws = open(b"- \n");
    let a = first(&ws);
    let mut h = History::new();
    let t0 = Instant::now();
    for (i, c) in "hello".chars().enumerate() {
        let tx = typed(&mut ws, a, i, &c.to_string(), t0, (i as u64) * 50);
        h.push(tx);
    }
    assert_eq!(h.undo_len(), 1);
    h.undo(&mut ws).expect("undo");
    assert_eq!(ser(&ws), b"- \n");
}
