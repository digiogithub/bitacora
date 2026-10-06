//! Property tests for the editing core (BIT-T-0136): random command sequences, inverse restores
//! the exact serialized bytes, and every fixture page round-trips through the model.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bitacora_core::editor::{BlockId, Cmd, Op, Target, Transaction, Workspace};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;
use proptest::prelude::*;

fn key() -> PageKey {
    PageKey::from_title("p")
}

fn open(src: &[u8]) -> Workspace {
    let mut ws = Workspace::new();
    ws.load_page(key(), "p", GraphPath::new("pages/p.md").ok(), src);
    ws
}

fn ser(ws: &Workspace) -> Vec<u8> {
    ws.page(&key()).expect("page").serialize()
}

fn shape(ws: &Workspace) -> Vec<(usize, String)> {
    let p = ws.page(&key()).expect("page");
    p.dfs()
        .iter()
        .map(|id| (p.depth_of(*id), p.block(*id).expect("b").text.clone()))
        .collect()
}

const CORPUS: &[&str] = &[
    "- a\n\t- a1\n\t\t- a11\n\t- a2\n- b\n- c\n\t- c1\n",
    "title:: T\nalias:: x\n\n- a\n  id:: 11111111-2222-3333-4444-555555555555\n- b\n  collapsed:: true\n\t- b1\n",
    "- a\r\n  - a1\r\n  - a2\r\n- b\r\n",
    "* a\n    * a1\n        * a11\n    * a2\n* b\n",
    "- a\n\n\n- b\n\t- b1\n\n- c",
    "- ```\n  - not a block\n  ```\n- after\n",
    "\u{feff}- ünï\n\t- cödé\n",
    "just text\n",
    "",
];

fn interpret(ws: &Workspace, kind: u8, a: usize, b: usize, text: &str) -> Option<Cmd> {
    let order = ws.page(&key())?.dfs();
    let pick = |i: usize| -> Option<BlockId> { order.get(i % order.len().max(1)).copied() };
    Some(match kind % 8 {
        0 => Cmd::SetText {
            id: pick(a)?,
            text: text.to_owned(),
        },
        1 => Cmd::InsertSibling {
            after: pick(a)?,
            text: text.to_owned(),
        },
        2 => Cmd::InsertChild {
            page: key(),
            parent: if order.is_empty() || b.is_multiple_of(3) {
                None
            } else {
                pick(a)
            },
            text: text.to_owned(),
        },
        3 => Cmd::DeleteBlocks {
            ids: vec![pick(a)?],
        },
        4 => Cmd::Indent {
            ids: vec![pick(a)?],
        },
        5 => Cmd::Outdent {
            ids: vec![pick(a)?],
        },
        6 => Cmd::MoveBlocks {
            ids: vec![pick(a)?],
            target: match b % 4 {
                0 => Target::Before(pick(b / 4)?),
                1 => Target::After(pick(b / 4)?),
                2 => Target::FirstChild(pick(b / 4)?),
                _ => Target::LastChild(pick(b / 4)?),
            },
        },
        _ => Cmd::SetCollapsed {
            ids: vec![pick(a)?],
            collapsed: b.is_multiple_of(2),
        },
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(160))]

    #[test]
    fn random_commands_invert_to_the_exact_original_bytes(
        src in 0..CORPUS.len(),
        steps in proptest::collection::vec((any::<u8>(), 0usize..64, 0usize..64, "[a-zA-Z0-9 äé\\n:]{0,12}"), 1..14),
    ) {
        let original = CORPUS[src].as_bytes();
        let mut ws = open(original);
        let base_bytes = ser(&ws);
        prop_assert_eq!(&base_bytes[..], original);
        let base_shape = shape(&ws);
        let mut txs: Vec<Transaction> = Vec::new();
        for (kind, a, b, text) in &steps {
            let Some(cmd) = interpret(&ws, *kind, *a, *b, text) else { continue };
            let before = ser(&ws);
            let Ok(tx) = ws.run("step", &cmd) else {
                // A refused or rolled-back command leaves the bytes untouched.
                prop_assert_eq!(ser(&ws), before);
                continue;
            };
            let after = ser(&ws);
            // What we serialize always re-parses to the model.
            let reparsed = bitacora_markdown::Document::parse(after.clone());
            prop_assert_eq!(reparsed.blocks.len(), shape(&ws).len());
            // Undo + redo of the single step is exact.
            let undo = ws.undo(&tx).expect("undo");
            prop_assert_eq!(ser(&ws), before);
            ws.undo(&undo).expect("redo");
            prop_assert_eq!(ser(&ws), after);
            txs.push(tx);
        }
        // Undo everything in reverse order: the original bytes come back exactly.
        for tx in txs.iter().rev() {
            ws.undo(tx).expect("undo all");
        }
        prop_assert_eq!(ser(&ws), base_bytes);
        prop_assert_eq!(shape(&ws), base_shape);
    }

    #[test]
    fn raw_text_ops_roundtrip(
        src in 0..CORPUS.len(),
        edits in proptest::collection::vec((0usize..32, 0usize..8, 0usize..8, "[a-z äé]{0,6}"), 1..8),
    ) {
        let mut ws = open(CORPUS[src].as_bytes());
        let base = ser(&ws);
        let mut ops: Vec<Op> = Vec::new();
        for (i, s, e, ins) in &edits {
            let order = ws.page(&key()).expect("p").dfs();
            if order.is_empty() { break }
            let id = order[i % order.len()];
            let text = ws.block(id).expect("b").text.clone();
            let (mut s, mut e) = ((*s).min(text.len()), (*e).min(text.len()));
            if s > e { std::mem::swap(&mut s, &mut e) }
            while !text.is_char_boundary(s) { s -= 1 }
            while !text.is_char_boundary(e) { e += 1 }
            let mut op = Op::EditText { id, range: s..e, removed: text[s..e].to_owned(), inserted: ins.clone() };
            // Text that would re-parse as extra blocks (e.g. breaking a code fence) is refused.
            if op.apply(&mut ws).is_err() {
                continue;
            }
            ops.push(op);
        }
        for op in ops.iter().rev() {
            op.inverse().expect("inverse").apply(&mut ws).expect("apply inverse");
        }
        prop_assert_eq!(ser(&ws), base);
    }
}

#[test]
fn every_fixture_page_roundtrips_through_the_model() {
    let mut count = 0;
    for name in bitacora_testkit::graph_names() {
        for file in bitacora_testkit::markdown_files(&name) {
            let bytes = std::fs::read(&file).expect("read");
            let ws = open(&bytes);
            let p = ws.page(&key()).expect("page");
            assert!(
                p.dfs().iter().all(|id| p.is_clean(*id)),
                "{} not clean after load",
                file.display()
            );
            assert_eq!(ser(&ws), bytes, "{}", file.display());
            count += 1;
        }
    }
    assert!(count > 10, "fixtures not found ({count})");
}
