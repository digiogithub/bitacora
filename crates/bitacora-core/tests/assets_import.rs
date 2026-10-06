//! Pasted and dropped files become attachments through the command queue's `Cmd`
//! (BIT-US-0096, BIT-SP-0002.R15).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use bitacora_core::assets::{asset_link, asset_path};
use bitacora_core::editor::{Cmd, FileStore, FsStore, MemStore, NewAsset, Workspace};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;

fn gp(s: &str) -> GraphPath {
    GraphPath::new(s).expect("path")
}

fn setup() -> (Workspace, MemStore, bitacora_core::editor::BlockId) {
    let mut ws = Workspace::new();
    let mut store = MemStore::default();
    store
        .files
        .insert(gp("journals/2025_11_14.md"), b"- note\n".to_vec());
    ws.load_page(
        PageKey::from_title("Nov 14th, 2025"),
        "Nov 14th, 2025",
        Some(gp("journals/2025_11_14.md")),
        b"- note\n",
    );
    let page = ws.page(&PageKey::from_title("Nov 14th, 2025")).unwrap();
    let id = page.roots[0];
    (ws, store, id)
}

fn asset(name: &str, index: usize, bytes: &[u8]) -> NewAsset {
    let path = asset_path(name, 1_731_580_000_000, index).unwrap();
    NewAsset {
        link: asset_link(Some(&gp("journals/2025_11_14.md")), &path, name),
        path,
        bytes: Arc::from(bytes),
    }
}

#[test]
fn importing_writes_the_files_and_the_links_in_one_transaction() {
    let (mut ws, mut store, id) = setup();
    let tx = ws
        .run(
            "Paste files",
            &Cmd::ImportAssets {
                target: id,
                cursor: 4..4,
                assets: vec![
                    asset("Screen Shot 2024.png", 0, &[1, 2, 3]),
                    asset("report 50%.docx", 1, &[9]),
                ],
            },
        )
        .unwrap();
    let page = ws.page(&PageKey::from_title("Nov 14th, 2025")).unwrap();
    assert_eq!(
        page.block(id).unwrap().text,
        "note![Screen Shot 2024.png](../assets/Screen_Shot_2024_1731580000000_0.png)\n[report 50%.docx](../assets/report_50_1731580000000_1.docx)"
    );
    // Nothing is on disk before the flush; then both files and the page are written.
    assert!(
        !store
            .files
            .contains_key(&gp("assets/report_50_1731580000000_1.docx"))
    );
    assert!(ws.flush(&mut store).is_complete());
    assert_eq!(
        store.files[&gp("assets/Screen_Shot_2024_1731580000000_0.png")],
        [1, 2, 3]
    );
    assert_eq!(
        store.files[&gp("assets/report_50_1731580000000_1.docx")],
        [9]
    );
    assert!(ws.pending_creates().is_empty());
    // One undo removes the text and recycles the files.
    ws.undo(&tx).unwrap();
    assert!(ws.flush(&mut store).is_complete());
    assert_eq!(store.files[&gp("journals/2025_11_14.md")], b"- note\n");
    assert!(
        !store
            .files
            .contains_key(&gp("assets/Screen_Shot_2024_1731580000000_0.png"))
    );
    assert!(store.files.contains_key(&gp(
        "logseq/.recycle/assets_Screen_Shot_2024_1731580000000_0.png"
    )));
}

#[test]
fn undo_before_the_flush_never_creates_the_file() {
    let (mut ws, mut store, id) = setup();
    let tx = ws
        .run(
            "Paste files",
            &Cmd::ImportAssets {
                target: id,
                cursor: 0..0,
                assets: vec![asset("a.png", 0, &[1])],
            },
        )
        .unwrap();
    ws.undo(&tx).unwrap();
    assert!(ws.pending_creates().is_empty());
    assert!(ws.flush(&mut store).is_complete());
    assert!(
        !store
            .files
            .keys()
            .any(|p| p.as_str().starts_with("assets/") || p.as_str().contains(".recycle"))
    );
}

#[test]
fn the_real_store_creates_the_assets_folder_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = FsStore::new(dir.path());
    store
        .write(&gp("assets/x_1_0.png"), &[7, 7])
        .expect("write");
    assert_eq!(
        std::fs::read(dir.path().join("assets/x_1_0.png")).unwrap(),
        [7, 7]
    );
    let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("assets"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers.len(), 1, "no temp file left: {leftovers:?}");
}
