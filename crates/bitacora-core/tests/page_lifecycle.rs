//! Lazy page creation, today's journal, recycle-on-delete and new graphs
//! (BIT-US-0028, BIT-US-0057, BIT-US-0089, BIT-US-0099).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bitacora_config::{ConfigEditor, EffectiveConfig};
use bitacora_core::date::Date;
use bitacora_core::editor::{Cmd, FileStore, FsStore, MemStore, Opened, Workspace};
use bitacora_core::graph::{Graph, PageKey};
use bitacora_core::graph_path::GraphPath;
use bitacora_core::new_graph::{NewGraphError, NewGraphOutcome, create_graph};

fn gp(s: &str) -> GraphPath {
    GraphPath::new(s).expect("path")
}

fn cfg(src: &str) -> EffectiveConfig {
    EffectiveConfig::from_texts(None, Some(src))
}

fn tlb() -> EffectiveConfig {
    cfg("{:file/name-format :triple-lowbar}")
}

fn type_into_first_block(ws: &mut Workspace, key: &PageKey, text: &str) {
    let id = ws.page(key).expect("page").roots[0];
    ws.run(
        "type",
        &Cmd::SetText {
            id,
            text: text.into(),
        },
    )
    .expect("set text");
}

fn files(store: &MemStore) -> Vec<String> {
    store.files.keys().map(ToString::to_string).collect()
}

// ---- BIT-US-0028 ------------------------------------------------------------------------

#[test]
fn navigating_creates_no_file() {
    let mut ws = Workspace::new();
    let mut store = MemStore::default();
    let o = ws
        .open_page("New Page", &tlb(), None, &store)
        .expect("open");
    assert!(matches!(o, Opened::Virtual(_)));
    // Opening again (navigating back) is the same page.
    let again = ws
        .open_page("new page", &tlb(), None, &store)
        .expect("open");
    assert_eq!(again, Opened::Existing(o.key().clone()));
    let page = ws.page(o.key()).expect("page");
    assert!(page.is_virtual());
    assert!(!page.needs_write());
    assert!(ws.dirty_pages().is_empty());
    let r = ws.flush(&mut store);
    assert!(r.is_complete());
    assert!(r.written.is_empty());
    assert!(store.files.is_empty());
}

#[test]
fn typing_creates_the_leaf_file_only() {
    let mut ws = Workspace::new();
    let mut store = MemStore::default();
    let o = ws
        .open_page("Projects/Bitacora", &tlb(), None, &store)
        .expect("open");
    // Typing blank text still creates nothing.
    type_into_first_block(&mut ws, o.key(), "  ");
    assert!(ws.flush(&mut store).written.is_empty());
    type_into_first_block(&mut ws, o.key(), "hello");
    let r = ws.flush(&mut store);
    assert!(r.is_complete());
    assert_eq!(files(&store), ["pages/Projects___Bitacora.md"]);
    assert_eq!(
        store.files[&gp("pages/Projects___Bitacora.md")],
        b"- hello\n"
    );
    assert!(!ws.page(o.key()).expect("page").is_virtual());
}

#[test]
fn legacy_graph_writes_title_property_for_lossy_names() {
    let mut ws = Workspace::new();
    let mut store = MemStore::default();
    let legacy = cfg("{}");
    let o = ws
        .open_page("Version 1.0", &legacy, None, &store)
        .expect("open");
    // The automatic title:: line alone is not content.
    assert!(ws.flush(&mut store).written.is_empty());
    type_into_first_block(&mut ws, o.key(), "hello");
    assert!(ws.flush(&mut store).is_complete());
    let path = files(&store).pop().expect("one file");
    assert_eq!(
        String::from_utf8(store.files[&gp(&path)].clone()).unwrap(),
        "title:: Version 1.0\n\n- hello\n"
    );
    // A title that round-trips needs no property.
    let o = ws.open_page("Plain", &legacy, None, &store).expect("open");
    type_into_first_block(&mut ws, o.key(), "x");
    ws.flush(&mut store);
    assert_eq!(store.files[&gp("pages/Plain.md")], b"- x\n");
}

#[test]
fn file_name_keeps_title_case_and_output_is_clean() {
    let mut ws = Workspace::new();
    let mut store = MemStore::default();
    let o = ws
        .open_page("Mi Página", &tlb(), None, &store)
        .expect("open");
    type_into_first_block(&mut ws, o.key(), "ñandú");
    ws.flush(&mut store);
    let bytes = &store.files[&gp("pages/Mi Página.md")];
    assert_eq!(bytes, "- ñandú\n".as_bytes());
    assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert!(!bytes.contains(&b'\r'));
}

#[test]
fn org_pages_and_aliases_get_no_twin() {
    let c = tlb();
    let notes = gp("pages/Notes.org");
    let owner = gp("pages/Owner.md");
    let g = Graph::from_texts(
        [
            (&notes, "* heading\n"),
            (&owner, "alias:: Nickname\n\n- body\n"),
        ],
        &c,
    );
    let mut store = MemStore::default();
    store.files.insert(notes.clone(), b"* heading\n".to_vec());
    store
        .files
        .insert(owner.clone(), b"alias:: Nickname\n\n- body\n".to_vec());
    let mut ws = Workspace::new();
    let o = ws.open_page("Notes", &c, Some(&g), &store).expect("open");
    assert!(matches!(o, Opened::Existing(_)));
    assert!(ws.page(o.key()).expect("page").read_only);
    // Opening an alias lands on the page that declared it.
    let a = ws
        .open_page("Nickname", &c, Some(&g), &store)
        .expect("open");
    assert_eq!(a, Opened::Existing(PageKey::from_title("Owner")));
    assert!(ws.flush(&mut store).written.is_empty());
    assert_eq!(store.files.len(), 2);
    assert!(!store.files.contains_key(&gp("pages/Notes.md")));
    assert!(!store.files.contains_key(&gp("pages/Nickname.md")));
}

#[test]
fn existing_file_that_was_not_loaded_is_loaded_not_overwritten() {
    let mut ws = Workspace::new();
    let mut store = MemStore::default();
    store
        .files
        .insert(gp("pages/Late.md"), b"- on disk\n".to_vec());
    let o = ws.open_page("Late", &tlb(), None, &store).expect("open");
    assert!(matches!(o, Opened::Existing(_)));
    assert_eq!(
        ws.page(o.key()).expect("page").serialize(),
        b"- on disk\n".to_vec()
    );
}

#[test]
fn page_property_goes_into_the_pre_block() {
    let mut ws = Workspace::new();
    let k = PageKey::from_title("p");
    ws.load_page(k.clone(), "p", Some(gp("pages/p.md")), b"- a\n");
    ws.run(
        "prop",
        &Cmd::SetPageProperty {
            page: k.clone(),
            key: "tags".into(),
            value: "demo".into(),
        },
    )
    .expect("set");
    let mut store = MemStore::default();
    store.files.insert(gp("pages/p.md"), b"- a\n".to_vec());
    assert!(ws.flush(&mut store).is_complete());
    assert_eq!(store.files[&gp("pages/p.md")], b"tags:: demo\n\n- a\n");
    // A second property joins the first; changing the value is in place.
    ws.run(
        "prop",
        &Cmd::SetPageProperty {
            page: k.clone(),
            key: "type".into(),
            value: "x".into(),
        },
    )
    .expect("set");
    ws.run(
        "prop",
        &Cmd::SetPageProperty {
            page: k.clone(),
            key: "tags".into(),
            value: "other".into(),
        },
    )
    .expect("set");
    assert!(ws.flush(&mut store).is_complete());
    assert_eq!(
        store.files[&gp("pages/p.md")],
        b"tags:: other\ntype:: x\n\n- a\n"
    );
}

#[test]
fn page_property_in_front_matter_page_stays_front_matter() {
    let src = "---\ntitle: P\n---\n- a\n";
    let mut ws = Workspace::new();
    let k = PageKey::from_title("p");
    ws.load_page(k.clone(), "p", Some(gp("pages/p.md")), src.as_bytes());
    ws.run(
        "prop",
        &Cmd::SetPageProperty {
            page: k.clone(),
            key: "tags".into(),
            value: "demo".into(),
        },
    )
    .expect("set");
    let mut store = MemStore::default();
    store
        .files
        .insert(gp("pages/p.md"), src.as_bytes().to_vec());
    assert!(ws.flush(&mut store).is_complete());
    assert_eq!(
        String::from_utf8(store.files[&gp("pages/p.md")].clone()).unwrap(),
        "---\ntitle: P\ntags: demo\n---\n\n- a\n"
    );
}

#[test]
fn page_property_on_virtual_page_creates_the_file() {
    let mut ws = Workspace::new();
    let mut store = MemStore::default();
    let o = ws.open_page("Props", &tlb(), None, &store).expect("open");
    ws.run(
        "prop",
        &Cmd::SetPageProperty {
            page: o.key().clone(),
            key: "type".into(),
            value: "idea".into(),
        },
    )
    .expect("set");
    assert!(ws.flush(&mut store).is_complete());
    assert!(
        String::from_utf8(store.files[&gp("pages/Props.md")].clone())
            .unwrap()
            .starts_with("type:: idea\n")
    );
}

// ---- BIT-US-0057 ------------------------------------------------------------------------

fn nov14() -> Date {
    Date::new(2025, 11, 14).expect("date")
}

#[test]
fn today_is_virtual_until_typed() {
    let c = tlb();
    let mut ws = Workspace::new();
    let mut store = MemStore::default();
    let o = ws
        .ensure_today(nov14(), &c, &store)
        .expect("ensure")
        .expect("enabled");
    assert_eq!(o.key(), &PageKey::from_title("Nov 14th, 2025"));
    assert_eq!(ws.page(o.key()).expect("page").title, "Nov 14th, 2025");
    assert!(ws.flush(&mut store).written.is_empty());
    assert!(store.files.is_empty());
    // Idempotent.
    assert!(matches!(
        ws.ensure_today(nov14(), &c, &store).unwrap().unwrap(),
        Opened::Existing(_)
    ));
    type_into_first_block(&mut ws, o.key(), "hello");
    assert!(ws.flush(&mut store).is_complete());
    assert_eq!(files(&store), ["journals/2025_11_14.md"]);
    assert_eq!(store.files[&gp("journals/2025_11_14.md")], b"- hello\n");
}

#[test]
fn journal_directory_and_file_format_are_configurable() {
    let c = cfg(r#"{:journals-directory "daily" :journal/file-name-format "yyyy-MM-dd"}"#);
    let mut ws = Workspace::new();
    let mut store = MemStore::default();
    let o = ws.ensure_today(nov14(), &c, &store).unwrap().unwrap();
    type_into_first_block(&mut ws, o.key(), "hello");
    ws.flush(&mut store);
    assert_eq!(files(&store), ["daily/2025-11-14.md"]);
}

#[test]
fn disabled_journals_are_not_created() {
    let c = cfg("{:feature/enable-journals? false}");
    let mut ws = Workspace::new();
    let store = MemStore::default();
    assert_eq!(ws.ensure_today(nov14(), &c, &store).unwrap(), None);
    assert_eq!(ws.pages().count(), 0);
}

#[test]
fn existing_journal_file_is_loaded_not_replaced() {
    let c = tlb();
    let mut ws = Workspace::new();
    let mut store = MemStore::default();
    store
        .files
        .insert(gp("journals/2025_11_14.md"), b"- earlier\n".to_vec());
    let o = ws.ensure_today(nov14(), &c, &store).unwrap().unwrap();
    assert!(matches!(o, Opened::Existing(_)));
    assert_eq!(
        ws.page(o.key()).unwrap().serialize(),
        b"- earlier\n".to_vec()
    );
}

fn template_workspace() -> Workspace {
    let mut ws = Workspace::new();
    let k = PageKey::from_title("templates");
    ws.load_page(
        k,
        "templates",
        Some(gp("pages/templates.md")),
        b"- daily\n  template:: daily\n  - Plan for <% today %>\n  - Notes\n- other\n  template:: other\n  - nope\n",
    );
    ws
}

#[test]
fn template_is_shown_and_journal_stays_pristine_until_changed() {
    let c = cfg(r#"{:file/name-format :triple-lowbar :default-templates {:journals "Daily"}}"#);
    let mut ws = template_workspace();
    let mut store = MemStore::default();
    let o = ws.ensure_today(nov14(), &c, &store).unwrap().unwrap();
    let page = ws.page(o.key()).unwrap();
    let texts: Vec<String> = page
        .dfs()
        .iter()
        .map(|id| page.block(*id).unwrap().text.clone())
        .collect();
    assert_eq!(texts, ["Plan for [[Nov 14th, 2025]]", "Notes"]);
    assert!(page.is_virtual());
    let notes = page.roots[1];
    // Nothing is written for the untouched template, not even after a flush.
    assert!(ws.flush(&mut store).written.is_empty());
    assert!(!store.files.contains_key(&gp("journals/2025_11_14.md")));
    // Changing the template content makes it a real journal.
    ws.run(
        "edit",
        &Cmd::SetText {
            id: notes,
            text: "Notes: met Ana".into(),
        },
    )
    .unwrap();
    assert!(ws.flush(&mut store).is_complete());
    assert_eq!(
        store.files[&gp("journals/2025_11_14.md")],
        b"- Plan for [[Nov 14th, 2025]]\n- Notes: met Ana\n"
    );
}

#[test]
fn unknown_template_falls_back_to_an_empty_first_block() {
    let c = cfg(r#"{:default-templates {:journals "missing"}}"#);
    let mut ws = Workspace::new();
    let store = MemStore::default();
    let o = ws.ensure_today(nov14(), &c, &store).unwrap().unwrap();
    assert_eq!(ws.page(o.key()).unwrap().roots.len(), 1);
}

#[test]
fn template_including_parent_keeps_the_block_without_template_properties() {
    let mut ws = Workspace::new();
    ws.load_page(
        PageKey::from_title("t"),
        "t",
        Some(gp("pages/t.md")),
        b"- Header\n  template:: hdr\n  template-including-parent:: true\n  - child\n",
    );
    let tpl = ws.find_template("HDR").expect("template");
    assert_eq!(tpl.len(), 1);
    assert_eq!(tpl[0].text, "Header");
    assert_eq!(tpl[0].children[0].text, "child");
}

#[test]
fn journal_pages_navigated_to_have_no_template_and_no_file() {
    let c = cfg(r#"{:default-templates {:journals "Daily"}}"#);
    let mut ws = template_workspace();
    let mut store = MemStore::default();
    let o = ws.open_page("Nov 1st, 2025", &c, None, &store).unwrap();
    assert_eq!(ws.page(o.key()).unwrap().roots.len(), 1);
    assert!(ws.flush(&mut store).written.is_empty());
}

// ---- BIT-US-0089 ------------------------------------------------------------------------

fn loaded(ws: &mut Workspace, store: &mut MemStore, title: &str, path: &str, text: &str) {
    ws.load_page(
        PageKey::from_title(title),
        title,
        Some(gp(path)),
        text.as_bytes(),
    );
    store.files.insert(gp(path), text.as_bytes().to_vec());
}

#[test]
fn deleting_a_page_recycles_its_file() {
    let mut ws = Workspace::new();
    let mut store = MemStore::default();
    loaded(&mut ws, &mut store, "foo", "pages/foo.md", "- foo\n");
    loaded(
        &mut ws,
        &mut store,
        "sub/bar",
        "pages/sub/bar.md",
        "- bar\n",
    );
    loaded(
        &mut ws,
        &mut store,
        "ref",
        "pages/ref.md",
        "- see [[foo]]\n",
    );
    // An older recycled copy is overwritten.
    store
        .files
        .insert(gp("logseq/.recycle/pages_foo.md"), b"- old\n".to_vec());
    ws.run(
        "delete",
        &Cmd::DeletePage {
            page: PageKey::from_title("foo"),
        },
    )
    .unwrap();
    ws.run(
        "delete",
        &Cmd::DeletePage {
            page: PageKey::from_title("sub/bar"),
        },
    )
    .unwrap();
    let r = ws.flush(&mut store);
    assert!(r.is_complete(), "{r:?}");
    assert_eq!(r.recycled.len(), 2);
    assert!(!store.files.contains_key(&gp("pages/foo.md")));
    assert_eq!(store.files[&gp("logseq/.recycle/pages_foo.md")], b"- foo\n");
    assert_eq!(
        store.files[&gp("logseq/.recycle/pages_sub_bar.md")],
        b"- bar\n"
    );
    // References are untouched.
    assert_eq!(store.files[&gp("pages/ref.md")], b"- see [[foo]]\n");
}

#[test]
fn deleted_page_that_is_referenced_or_aliased_survives_as_a_virtual_entity() {
    let c = tlb();
    let ref_ = gp("pages/ref.md");
    let owner = gp("pages/owner.md");
    let g = Graph::from_texts(
        [(&ref_, "- see [[foo]]\n"), (&owner, "alias:: foo\n\n- b\n")],
        &c,
    );
    let foo = g.page(&PageKey::from_title("foo")).expect("alias entity");
    assert!(foo.is_virtual());
}

#[test]
fn deleting_a_virtual_page_touches_no_file() {
    let mut ws = Workspace::new();
    let mut store = MemStore::default();
    let o = ws.open_page("Ghost", &tlb(), None, &store).unwrap();
    ws.run(
        "delete",
        &Cmd::DeletePage {
            page: o.key().clone(),
        },
    )
    .unwrap();
    assert!(ws.flush(&mut store).recycled.is_empty());
    assert!(store.files.is_empty());
}

#[test]
fn delete_page_is_undoable() {
    let mut ws = Workspace::new();
    let mut store = MemStore::default();
    loaded(&mut ws, &mut store, "foo", "pages/foo.md", "- foo\n");
    let tx = ws
        .run(
            "delete",
            &Cmd::DeletePage {
                page: PageKey::from_title("foo"),
            },
        )
        .unwrap();
    // Undo before the flush: nothing ever happens to the file.
    ws.undo(&tx).unwrap();
    assert!(ws.flush(&mut store).recycled.is_empty());
    assert_eq!(store.files[&gp("pages/foo.md")], b"- foo\n");
    // Undo after the flush: the page file is back.
    let tx = ws
        .run(
            "delete",
            &Cmd::DeletePage {
                page: PageKey::from_title("foo"),
            },
        )
        .unwrap();
    assert!(ws.flush(&mut store).is_complete());
    assert!(!store.files.contains_key(&gp("pages/foo.md")));
    ws.undo(&tx).unwrap();
    let r = ws.flush(&mut store);
    assert!(r.is_complete(), "{r:?}");
    assert_eq!(store.files[&gp("pages/foo.md")], b"- foo\n");
}

#[test]
fn favorites_cleanup_is_a_surgical_config_edit() {
    let src = ";; mine\n{:meta/version 1\n :favorites [\"foo\" \"keep\"] ; pinned\n :hidden []}\n";
    let mut ed = ConfigEditor::parse(src).unwrap();
    assert!(ed.favorites_remove("foo").unwrap());
    assert_eq!(
        ed.text(),
        ";; mine\n{:meta/version 1\n :favorites [\"keep\"] ; pinned\n :hidden []}\n"
    );
}

#[test]
fn deleting_an_asset_recycles_it_and_undo_restores_it() {
    let mut ws = Workspace::new();
    let mut store = MemStore::default();
    store.files.insert(gp("assets/x.png"), vec![1, 2, 3]);
    let tx = ws
        .run(
            "delete asset",
            &Cmd::DeleteAsset {
                path: gp("assets/x.png"),
            },
        )
        .unwrap();
    let r = ws.flush(&mut store);
    assert!(r.is_complete(), "{r:?}");
    assert!(!store.files.contains_key(&gp("assets/x.png")));
    assert_eq!(store.files[&gp("logseq/.recycle/assets_x.png")], [1, 2, 3]);
    ws.undo(&tx).unwrap();
    assert!(ws.flush(&mut store).is_complete());
    assert_eq!(store.files[&gp("assets/x.png")], [1, 2, 3]);
    assert!(
        !store
            .files
            .contains_key(&gp("logseq/.recycle/assets_x.png"))
    );
}

#[test]
fn asset_delete_refuses_page_files() {
    let mut ws = Workspace::new();
    let mut store = MemStore::default();
    loaded(&mut ws, &mut store, "foo", "pages/foo.md", "- foo\n");
    assert!(
        ws.run(
            "x",
            &Cmd::DeleteAsset {
                path: gp("pages/foo.md")
            }
        )
        .is_err()
    );
}

#[test]
fn fs_store_recycles_with_rename_and_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("pages/sub")).unwrap();
    std::fs::write(root.join("pages/sub/bar.md"), "- bar\n").unwrap();
    std::fs::create_dir_all(root.join("logseq/.recycle")).unwrap();
    std::fs::write(root.join("logseq/.recycle/pages_sub_bar.md"), "- old\n").unwrap();
    let mut store = FsStore::new(root);
    let dest = store.recycle(&gp("pages/sub/bar.md")).unwrap().unwrap();
    assert_eq!(dest.as_str(), "logseq/.recycle/pages_sub_bar.md");
    assert!(!root.join("pages/sub/bar.md").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("logseq/.recycle/pages_sub_bar.md")).unwrap(),
        "- bar\n"
    );
    assert_eq!(store.recycle(&gp("pages/missing.md")).unwrap(), None);
    assert!(store.unrecycle(&gp("pages/sub/bar.md")).unwrap());
    assert_eq!(
        std::fs::read_to_string(root.join("pages/sub/bar.md")).unwrap(),
        "- bar\n"
    );
}

// ---- BIT-US-0099 ------------------------------------------------------------------------

#[test]
fn new_graph_layout_and_config() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("my-graph");
    assert_eq!(create_graph(&root).unwrap(), NewGraphOutcome::Created);
    assert_eq!(
        std::fs::read_to_string(root.join("pages/contents.md")).unwrap(),
        "-\n"
    );
    assert!(root.join("journals").is_dir());
    assert!(root.join("logseq/.recycle").is_dir());
    assert_eq!(std::fs::read(root.join("logseq/custom.css")).unwrap(), b"");
    let c = EffectiveConfig::load(&root, None);
    assert!(c.diagnostics().is_empty(), "{:?}", c.diagnostics());
    assert_eq!(c.name_format(), bitacora_config::NameFormat::TripleLowbar);
    assert_eq!(c.meta_version(), 1);
    // The graph loads and has the contents page.
    let g = Graph::load(&root, &c).unwrap();
    assert!(g.page(&PageKey::from_title("contents")).is_some());
    // Opening it again changes nothing.
    std::fs::write(root.join("pages/contents.md"), "- mine\n").unwrap();
    assert_eq!(create_graph(&root).unwrap(), NewGraphOutcome::AlreadyGraph);
    assert_eq!(
        std::fs::read_to_string(root.join("pages/contents.md")).unwrap(),
        "- mine\n"
    );
}

#[test]
fn new_graph_refuses_a_non_empty_folder() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
    assert!(matches!(
        create_graph(dir.path()),
        Err(NewGraphError::NotEmpty(_))
    ));
    assert!(!dir.path().join("logseq").exists());
    // A fresh `git init` does not count as content.
    let d2 = tempfile::tempdir().unwrap();
    std::fs::create_dir(d2.path().join(".git")).unwrap();
    assert_eq!(create_graph(d2.path()).unwrap(), NewGraphOutcome::Created);
}
