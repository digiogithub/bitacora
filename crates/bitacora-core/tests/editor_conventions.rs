//! File conventions kept across undo (BIT-US-0039) and settings applied through the queue.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bitacora_core::editor::{Cmd, EditorSettings, History, MemStore, Workflow, Workspace};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;
use bitacora_core::queue::{CommandQueue, QueueConfig, Source};

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

#[test]
fn file_without_final_newline_keeps_that_convention_across_append_and_undo() {
    let src = b"- a\n- b";
    let mut ws = open(src);
    let mut store = MemStore::default();
    store.files.insert(path(), src.to_vec());
    let mut h = History::new();
    let b = ws.page(&key()).expect("p").roots[1];
    let tx = ws
        .run(
            "Add",
            &Cmd::InsertSibling {
                after: b,
                text: "x".into(),
            },
        )
        .expect("insert");
    h.push(tx);
    assert!(ws.flush(&mut store).is_complete());
    assert_eq!(store.files[&path()], b"- a\n- b\n- x");
    h.undo(&mut ws).expect("undo");
    assert!(ws.flush(&mut store).is_complete());
    assert_eq!(store.files[&path()], src);
}

#[test]
fn undo_after_a_write_restores_blank_lines_and_trailing_spaces_of_edited_blocks() {
    let src = b"- keep  \n\n\n- edit me  \n\n- last\n";
    let mut ws = open(src);
    let mut store = MemStore::default();
    store.files.insert(path(), src.to_vec());
    let mut h = History::new();
    let edit = ws.page(&key()).expect("p").roots[1];
    let tx = ws
        .run(
            "Edit",
            &Cmd::SetText {
                id: edit,
                text: "edited".into(),
            },
        )
        .expect("edit");
    h.push(tx);
    assert!(ws.flush(&mut store).is_complete());
    assert_eq!(store.files[&path()], b"- keep  \n\n\n- edited\n- last\n");
    h.undo(&mut ws).expect("undo");
    assert!(ws.flush(&mut store).is_complete());
    assert_eq!(
        store.files[&path()],
        src,
        "the old bytes come back verbatim"
    );
    h.redo(&mut ws).expect("redo");
    assert!(ws.flush(&mut store).is_complete());
    assert_eq!(store.files[&path()], b"- keep  \n\n\n- edited\n- last\n");
}

#[test]
fn queue_applies_editor_settings_to_planners() {
    let src = "- p\n\t- a\n\t- b\n";
    let mut ws = Workspace::new();
    ws.load_page(key(), "p", Some(path()), src.as_bytes());
    let (q, join) = CommandQueue::spawn(
        ws,
        Box::new(MemStore::default()),
        QueueConfig {
            debounce: None,
            ..QueueConfig::default()
        },
    );
    q.set_settings(
        Source::Ui,
        EditorSettings {
            logical_outdenting: true,
            workflow: Workflow::Todo,
        },
    )
    .expect("settings");
    let a = q.snapshot(&key()).expect("s").blocks[1].id;
    q.run(Source::Ui, "Outdent", Cmd::Outdent { ids: vec![a] })
        .expect("outdent");
    let ws = join.shutdown().expect("shutdown");
    assert_eq!(
        ws.page(&key()).expect("p").serialize(),
        b"- p\n\t- b\n- a\n"
    );
}
