//! Page rename, reference cascade and merge (BIT-US-0061, BIT-US-0082, BIT-US-0087) on temp
//! graphs: byte-exact untouched content, one undoable transaction, collision merge.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::Arc;

use bitacora_config::EffectiveConfig;
use bitacora_core::editor::{
    FileStore, FsStore, MemStore, MergeMode, PageFile, RefLookup, RenameError, RenameRequest,
    Transaction, Workspace,
};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;
use bitacora_core::naming::file_body_decode;
use bitacora_core::queue::{CommandQueue, QueueConfig, QueueError, Source};

const CONFIG: &str = "{:meta/version 1\n ;; favourites are renamed in place\n :favorites [\"Old\" \"Other\"]\n :default-home {:page \"Old\"}\n :file/name-format :triple-lowbar}\n";

fn gp(s: &str) -> GraphPath {
    GraphPath::new(s).expect("path")
}

fn cfg() -> Arc<EffectiveConfig> {
    Arc::new(EffectiveConfig::from_texts(None, Some(CONFIG)))
}

/// Index stand-in: scans `pages/` and returns every file whose text mentions one of the names
/// (case-insensitively) or lies below a namespace.
struct DirLookup(std::path::PathBuf);

impl DirLookup {
    fn all(&self) -> Vec<(PageFile, String)> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(self.0.join("pages")) else {
            return out;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(body) = name
                .strip_suffix(".md")
                .or_else(|| name.strip_suffix(".org"))
            else {
                continue;
            };
            let text = std::fs::read_to_string(e.path()).unwrap_or_default();
            out.push((
                PageFile {
                    title: file_body_decode(body, bitacora_config::NameFormat::TripleLowbar),
                    path: gp(&format!("pages/{name}")),
                },
                text,
            ));
        }
        out
    }
}

impl RefLookup for DirLookup {
    fn files_referencing(&self, names: &[String]) -> Result<Vec<PageFile>, String> {
        Ok(self
            .all()
            .into_iter()
            .filter(|(_, t)| {
                let t = t.to_lowercase();
                names.iter().any(|n| t.contains(&n.to_lowercase()))
            })
            .map(|(f, _)| f)
            .collect())
    }
    fn namespace_children(&self, title: &str) -> Result<Vec<PageFile>, String> {
        let prefix = format!("{}/", title.to_lowercase());
        Ok(self
            .all()
            .into_iter()
            .filter(|(f, _)| f.title.to_lowercase().starts_with(&prefix))
            .map(|(f, _)| f)
            .collect())
    }
    fn page_file(&self, title: &str) -> Result<Option<PageFile>, String> {
        Ok(self
            .all()
            .into_iter()
            .map(|(f, _)| f)
            .find(|f| f.title.to_lowercase() == title.to_lowercase()))
    }
}

struct Graph {
    dir: tempfile::TempDir,
    store: FsStore,
    ws: Workspace,
}

fn graph(files: &[(&str, &str)]) -> Graph {
    let dir = tempfile::tempdir().expect("tmp");
    for (p, c) in files {
        let abs = dir.path().join(p);
        std::fs::create_dir_all(abs.parent().expect("parent")).expect("mkdir");
        std::fs::write(abs, c).expect("seed");
    }
    std::fs::create_dir_all(dir.path().join("logseq")).expect("mkdir");
    std::fs::write(dir.path().join("logseq/config.edn"), CONFIG).expect("config");
    let store = FsStore::new(dir.path());
    Graph {
        dir,
        store,
        ws: Workspace::new(),
    }
}

impl Graph {
    fn req(&self, from: &str, to: &str) -> RenameRequest {
        RenameRequest {
            from: from.into(),
            to: to.into(),
            config: cfg(),
            lookup: Some(Arc::new(DirLookup(self.dir.path().to_path_buf()))),
            merge: MergeMode::Refuse,
        }
    }

    fn rename(&mut self, req: &RenameRequest) -> Result<Transaction, RenameError> {
        let plan = self.ws.plan_rename(&self.store, req)?;
        Ok(self.ws.commit("Rename page", plan.ops)?)
    }

    fn flush(&mut self) {
        let r = self.ws.flush(&mut self.store);
        assert!(r.is_complete(), "flush: {r:?}");
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.dir.path().join(rel)).unwrap_or_else(|_| "<missing>".into())
    }

    fn exists(&self, rel: &str) -> bool {
        self.dir.path().join(rel).exists()
    }

    /// Every file except `logseq/bak` (backups are an expected side effect).
    fn snapshot(&self) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        walk(self.dir.path(), self.dir.path(), &mut out);
        out
    }
}

fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
    for e in std::fs::read_dir(dir).expect("dir").flatten() {
        let p = e.path();
        let rel = p
            .strip_prefix(root)
            .expect("prefix")
            .to_string_lossy()
            .replace('\\', "/");
        if rel.starts_with("logseq/bak") {
            continue;
        }
        if p.is_dir() {
            walk(root, &p, out);
        } else {
            out.insert(rel, std::fs::read(&p).expect("read"));
        }
    }
}

const OLD_PAGE: &str = "- first block\n- second [[Old]] self reference\n  id:: 6500c1a4-0000-4000-8000-000000000001\n- third\n";
const OTHER: &str = "- untouched   block with  odd   spacing\n- see [[Old]] and #Old and `[[Old]]`\n  tags:: Old, x\n- plain\n";

// ---- BIT-US-0061 -------------------------------------------------------------------------

#[test]
fn rename_moves_file_rewrites_refs_and_config() {
    let mut g = graph(&[("pages/Old.md", OLD_PAGE), ("pages/Other.md", OTHER)]);
    let tx = g.rename(&g.req("Old", "New Idea")).expect("rename");
    assert!(!tx.ops.is_empty());
    g.flush();

    assert!(!g.exists("pages/Old.md"));
    // Only the self reference changed; the id:: line and the other blocks are byte-identical.
    assert_eq!(
        g.read("pages/New Idea.md"),
        OLD_PAGE.replace("[[Old]]", "[[New Idea]]")
    );
    assert_eq!(
        g.read("pages/Other.md"),
        "- untouched   block with  odd   spacing\n- see [[New Idea]] and #[[New Idea]] and `[[Old]]`\n  tags:: New Idea, x\n- plain\n"
    );
    // config.edn: favourites and default home renamed, everything else byte-identical.
    assert_eq!(
        g.read("logseq/config.edn"),
        CONFIG.replace("\"Old\"", "\"New Idea\"")
    );
    // Only the file moved: no other page file changed its name.
    let names: Vec<String> = g
        .snapshot()
        .keys()
        .filter(|k| k.starts_with("pages/"))
        .cloned()
        .collect();
    assert_eq!(names, ["pages/New Idea.md", "pages/Other.md"]);
}

#[test]
fn undo_restores_every_file_byte_for_byte() {
    let mut g = graph(&[("pages/Old.md", OLD_PAGE), ("pages/Other.md", OTHER)]);
    let before = g.snapshot();
    let tx = g.rename(&g.req("Old", "New Idea")).expect("rename");
    g.flush();
    assert_ne!(g.snapshot(), before);

    let undone = g.ws.undo(&tx).expect("undo");
    g.flush();
    assert_eq!(g.snapshot(), before);

    // Redo = undo of the undo.
    let again = g.ws.undo(&undone).expect("redo");
    g.flush();
    assert!(g.exists("pages/New Idea.md"));
    g.ws.undo(&again).expect("undo again");
    g.flush();
    assert_eq!(g.snapshot(), before);
}

#[test]
fn undo_before_flush_leaves_the_disk_alone() {
    let mut g = graph(&[("pages/Old.md", OLD_PAGE), ("pages/Other.md", OTHER)]);
    let before = g.snapshot();
    let tx = g.rename(&g.req("Old", "New")).expect("rename");
    g.ws.undo(&tx).expect("undo");
    g.flush();
    assert_eq!(g.snapshot(), before);
}

#[test]
fn namespace_children_follow_with_the_prefix_replaced_once() {
    let mut g = graph(&[
        ("pages/a___x.md", "- x refers [[a/y]]\n"),
        ("pages/a___y.md", "- y\n"),
        ("pages/a.md", "- root a\n"),
        ("pages/unrelated.md", "- [[a/x]] and [[a]]\n"),
        ("pages/ab.md", "- not a child\n"),
    ]);
    let rep = g.rename(&g.req("a", "b")).expect("rename");
    g.flush();
    let _ = rep;
    assert_eq!(g.read("pages/b___x.md"), "- x refers [[b/y]]\n");
    assert_eq!(g.read("pages/b___y.md"), "- y\n");
    assert_eq!(g.read("pages/b.md"), "- root a\n");
    assert_eq!(g.read("pages/unrelated.md"), "- [[b/x]] and [[b]]\n");
    assert_eq!(g.read("pages/ab.md"), "- not a child\n");
    for old in ["a___x", "a___y", "a"] {
        assert!(!g.exists(&format!("pages/{old}.md")), "{old} still there");
    }
}

#[test]
fn case_only_rename_renames_the_file() {
    let mut g = graph(&[("pages/foo.md", "- [[foo]] here\n")]);
    g.rename(&g.req("foo", "Foo")).expect("rename");
    g.flush();
    assert_eq!(g.read("pages/Foo.md"), "- [[Foo]] here\n");
    let names: Vec<_> = std::fs::read_dir(g.dir.path().join("pages"))
        .expect("dir")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["Foo.md"]);
}

/// A store that behaves like a case-insensitive file system.
#[derive(Default)]
struct CiStore(MemStore);

impl CiStore {
    fn key(p: &GraphPath) -> GraphPath {
        GraphPath::new(&p.as_str().to_lowercase()).expect("path")
    }
}

impl FileStore for CiStore {
    fn read(&self, path: &GraphPath) -> io::Result<Option<Vec<u8>>> {
        self.0.read(&Self::key(path))
    }
    fn write(&mut self, path: &GraphPath, bytes: &[u8]) -> io::Result<()> {
        self.0.write(&Self::key(path), bytes)
    }
    fn remove(&mut self, path: &GraphPath) -> io::Result<()> {
        self.0.remove(&Self::key(path))
    }
    fn list(&self, dir: &GraphPath) -> io::Result<Vec<String>> {
        self.0.list(&Self::key(dir))
    }
}

#[test]
fn case_only_rename_survives_a_case_insensitive_file_system() {
    let mut store = CiStore::default();
    store.write(&gp("pages/foo.md"), b"- body\n").expect("seed");
    let mut ws = Workspace::new();
    ws.load_page(
        PageKey::from_title("foo"),
        "foo",
        Some(gp("pages/foo.md")),
        b"- body\n",
    );
    let req = RenameRequest {
        from: "foo".into(),
        to: "Foo".into(),
        config: cfg(),
        lookup: None,
        merge: MergeMode::Refuse,
    };
    let plan = ws.plan_rename(&store, &req).expect("plan");
    ws.commit("Rename page", plan.ops).expect("commit");
    let r = ws.flush(&mut store);
    assert!(r.is_complete(), "{r:?}");
    // The data is still there under the (case-folded) name and no temp file is left behind.
    assert_eq!(
        store.read(&gp("pages/Foo.md")).expect("read"),
        Some(b"- body\n".to_vec())
    );
    assert!(
        store
            .0
            .files
            .keys()
            .all(|k| !k.as_str().ends_with(".bitacora-rename"))
    );
    assert_eq!(
        ws.page(&PageKey::from_title("foo")).expect("page").title,
        "Foo"
    );
}

#[test]
fn title_property_and_front_matter_title_are_rewritten() {
    let mut g = graph(&[
        ("pages/foo.md", "title:: foo\ntags:: x\n\n- body\n"),
        ("pages/fm.md", "---\ntitle: fm\n---\n\n- body\n"),
    ]);
    g.rename(&g.req("foo", "bar")).expect("rename foo");
    g.rename(&g.req("fm", "fm2")).expect("rename fm");
    g.flush();
    assert_eq!(g.read("pages/bar.md"), "title:: bar\ntags:: x\n\n- body\n");
    assert_eq!(g.read("pages/fm2.md"), "---\ntitle: fm2\n---\n\n- body\n");
}

#[test]
fn legacy_graph_gets_title_property_when_the_name_does_not_round_trip() {
    let legacy = Arc::new(EffectiveConfig::from_texts(None, Some("{}")));
    let mut store = MemStore::default();
    store.write(&gp("pages/a.md"), b"- body\n").expect("seed");
    let mut ws = Workspace::new();
    ws.load_page(
        PageKey::from_title("a"),
        "a",
        Some(gp("pages/a.md")),
        b"- body\n",
    );
    let req = RenameRequest {
        from: "a".into(),
        to: "a.b".into(),
        config: legacy,
        lookup: None,
        merge: MergeMode::Refuse,
    };
    let plan = ws.plan_rename(&store, &req).expect("plan");
    ws.commit("Rename page", plan.ops).expect("commit");
    assert!(ws.flush(&mut store).is_complete());
    let new_file = store
        .files
        .iter()
        .find(|(p, _)| p.as_str() != "logseq/config.edn")
        .expect("page file");
    assert!(
        String::from_utf8_lossy(new_file.1).starts_with("title:: a.b"),
        "{new_file:?}"
    );
}

#[test]
fn journals_are_never_renamed() {
    let mut g = graph(&[("journals/2025_01_02.md", "- day\n")]);
    let before = g.snapshot();
    let err = g
        .rename(&g.req("Jan 2nd, 2025", "Something"))
        .expect_err("refused");
    assert!(matches!(err, RenameError::Journal(_)), "{err:?}");
    g.flush();
    assert_eq!(g.snapshot(), before);
}

#[test]
fn blank_and_unchanged_titles_are_refused() {
    let mut g = graph(&[("pages/a.md", "- a\n")]);
    assert!(matches!(
        g.rename(&g.req("a", " ")),
        Err(RenameError::Blank)
    ));
    assert!(matches!(
        g.rename(&g.req("a", "a")),
        Err(RenameError::Unchanged)
    ));
}

#[test]
fn rename_to_an_existing_file_name_is_refused_without_changes() {
    // `A b` and `a b` are the same page key, so use a child whose new name is taken by a file the
    // workspace does not know as a page.
    let mut g = graph(&[
        ("pages/a___x.md", "- x\n"),
        ("pages/a.md", "- a\n"),
        ("pages/b___x.md", "- taken\n"),
    ]);
    let before = g.snapshot();
    let err = g.rename(&g.req("a", "b")).expect_err("collision");
    assert!(matches!(err, RenameError::ChildCollision(_)), "{err:?}");
    g.flush();
    assert_eq!(g.snapshot(), before);
}

#[test]
fn a_file_less_page_can_be_renamed_for_references_and_config() {
    let mut g = graph(&[("pages/Other.md", "- [[Ghost]] #Ghost\n")]);
    let plan =
        g.ws.plan_rename(&g.store, &g.req("Ghost", "Spirit"))
            .expect("plan");
    assert_eq!(plan.rewritten_blocks, 1);
    g.ws.commit("Rename page", plan.ops).expect("commit");
    g.flush();
    assert_eq!(g.read("pages/Other.md"), "- [[Spirit]] #Spirit\n");
}

#[test]
fn read_only_pages_are_reported_and_skipped() {
    let mut g = graph(&[
        ("pages/Old.md", "- old\n"),
        ("pages/legacy.org", "* see [[Old]]\n"),
    ]);
    let plan =
        g.ws.plan_rename(&g.store, &g.req("Old", "New"))
            .expect("plan");
    assert_eq!(plan.skipped_read_only, ["legacy"]);
    g.ws.commit("Rename page", plan.ops).expect("commit");
    g.flush();
    assert_eq!(g.read("pages/legacy.org"), "* see [[Old]]\n");
}

#[test]
fn a_file_changed_on_disk_is_never_overwritten() {
    let mut g = graph(&[("pages/Old.md", "- old\n"), ("pages/Other.md", OTHER)]);
    g.rename(&g.req("Old", "New")).expect("rename");
    // Somebody edits Other.md after we read it.
    let external = "- edited elsewhere [[Old]]\n";
    std::fs::write(g.dir.path().join("pages/Other.md"), external).expect("write");
    let r = g.ws.flush(&mut g.store);
    assert!(!r.is_complete());
    assert_eq!(g.read("pages/Other.md"), external);
}

// ---- BIT-US-0082 -------------------------------------------------------------------------

#[test]
fn page_properties_in_the_preamble_are_rewritten() {
    let mut g = graph(&[
        ("pages/Old.md", "- old\n"),
        (
            "pages/Tagged.md",
            "tags:: Old, other\nalias:: Old\n\n- body #old\n",
        ),
    ]);
    g.rename(&g.req("Old", "New Idea")).expect("rename");
    g.flush();
    assert_eq!(
        g.read("pages/Tagged.md"),
        "tags:: New Idea, other\nalias:: New Idea\n\n- body #[[New Idea]]\n"
    );
}

#[test]
fn property_keys_and_case_insensitive_refs_are_rewritten() {
    let mut g = graph(&[
        ("pages/Status.md", "- s\n"),
        (
            "pages/Use.md",
            "- t\n  status:: done\n- [[status]] [[STATUS]]\n",
        ),
    ]);
    g.rename(&g.req("Status", "Progress Report"))
        .expect("rename");
    g.flush();
    assert_eq!(
        g.read("pages/Use.md"),
        "- t\n  progress-report:: done\n- [[Progress Report]] [[Progress Report]]\n"
    );
}

// ---- BIT-US-0087 -------------------------------------------------------------------------

const FOO: &str =
    "- foo one\n  id:: 6500c1a4-0000-4000-8000-0000000000aa\n  - nested child\n- foo two [[Foo]]\n";
const BAR: &str = "- bar block\n";
const REFS: &str = "- see [[Foo]] #Foo and ((6500c1a4-0000-4000-8000-0000000000aa))\n";

fn merge_graph() -> Graph {
    graph(&[
        ("pages/Foo.md", FOO),
        ("pages/Bar.md", BAR),
        ("pages/Refs.md", REFS),
    ])
}

#[test]
fn renaming_onto_an_existing_page_needs_confirmation() {
    let mut g = merge_graph();
    let before = g.snapshot();
    let err = g.rename(&g.req("Foo", "Bar")).expect_err("refused");
    assert!(
        matches!(err, RenameError::TargetExists(ref t) if t == "Bar"),
        "{err:?}"
    );
    g.flush();
    assert_eq!(g.snapshot(), before);
}

#[test]
fn merge_appends_blocks_recycles_the_source_and_rewrites_refs() {
    let mut g = merge_graph();
    let mut req = g.req("Foo", "Bar");
    req.merge = MergeMode::Merge {
        keep_aliases: false,
    };
    let plan = g.ws.plan_rename(&g.store, &req).expect("plan");
    assert!(plan.merged);
    g.ws.commit("Rename page", plan.ops).expect("commit");
    g.flush();

    // Bar's own bytes come first, then Foo's blocks (ids and depth kept).
    assert_eq!(
        g.read("pages/Bar.md"),
        "- bar block\n- foo one\n  id:: 6500c1a4-0000-4000-8000-0000000000aa\n\t- nested child\n- foo two [[Bar]]\n"
    );
    assert!(!g.exists("pages/Foo.md"));
    assert_eq!(g.read("logseq/.recycle/pages_Foo.md"), FOO);
    assert_eq!(
        g.read("pages/Refs.md"),
        "- see [[Bar]] #Bar and ((6500c1a4-0000-4000-8000-0000000000aa))\n"
    );
    // Config entries of the merged page point at the target now.
    assert!(g.read("logseq/config.edn").contains("\"Other\""));
}

#[test]
fn merge_is_one_undoable_transaction() {
    let mut g = merge_graph();
    let before = g.snapshot();
    let mut req = g.req("Foo", "Bar");
    req.merge = MergeMode::Merge {
        keep_aliases: false,
    };
    let plan = g.ws.plan_rename(&g.store, &req).expect("plan");
    let tx = g.ws.commit("Rename page", plan.ops).expect("commit");
    g.flush();
    assert!(!g.exists("pages/Foo.md"));

    g.ws.undo(&tx).expect("undo");
    g.flush();
    let mut after = g.snapshot();
    // The recycle copy of the undone delete is moved back by the flush.
    after.retain(|k, _| !k.starts_with("logseq/.recycle"));
    let mut expected = before;
    expected.retain(|k, _| !k.starts_with("logseq/.recycle"));
    assert_eq!(after, expected);
}

#[test]
fn merge_drops_aliases_with_a_warning_unless_opted_in() {
    let files = [
        ("pages/Foo.md", "alias:: Fu, Phoo\n\n- foo\n"),
        ("pages/Bar.md", "alias:: Existing\n\n- bar\n"),
    ];
    let mut g = graph(&files);
    let mut req = g.req("Foo", "Bar");
    req.merge = MergeMode::Merge {
        keep_aliases: false,
    };
    let plan = g.ws.plan_rename(&g.store, &req).expect("plan");
    assert_eq!(plan.dropped_aliases, ["Fu", "Phoo"]);
    assert!(!plan.warnings.is_empty());
    g.ws.commit("Rename page", plan.ops).expect("commit");
    g.flush();
    assert_eq!(g.read("pages/Bar.md"), "alias:: Existing\n\n- bar\n- foo\n");

    let mut g = graph(&files);
    let mut req = g.req("Foo", "Bar");
    req.merge = MergeMode::Merge { keep_aliases: true };
    let plan = g.ws.plan_rename(&g.store, &req).expect("plan");
    assert!(plan.dropped_aliases.is_empty());
    g.ws.commit("Rename page", plan.ops).expect("commit");
    g.flush();
    assert_eq!(
        g.read("pages/Bar.md"),
        "alias:: Existing, Fu, Phoo\n\n- bar\n- foo\n"
    );
}

#[test]
fn merge_does_not_rename_namespace_children() {
    let mut g = graph(&[
        ("pages/Foo.md", "- foo\n"),
        ("pages/Foo___kid.md", "- kid\n"),
        ("pages/Bar.md", "- bar\n"),
        ("pages/Ref.md", "- [[Foo/kid]] [[Foo]]\n"),
    ]);
    let mut req = g.req("Foo", "Bar");
    req.merge = MergeMode::Merge {
        keep_aliases: false,
    };
    let plan = g.ws.plan_rename(&g.store, &req).expect("plan");
    g.ws.commit("Rename page", plan.ops).expect("commit");
    g.flush();
    assert_eq!(g.read("pages/Foo___kid.md"), "- kid\n");
    assert_eq!(g.read("pages/Ref.md"), "- [[Foo/kid]] [[Bar]]\n");
}

#[test]
fn merging_into_an_empty_in_memory_page_replaces_it() {
    let mut g = graph(&[("pages/Foo.md", "- foo\n")]);
    // `Bar` was opened (navigated to) but never written.
    let c = cfg();
    g.ws.open_page("Bar", &c, None, &g.store).expect("open");
    let plan =
        g.ws.plan_rename(&g.store, &g.req("Foo", "Bar"))
            .expect("plain rename");
    assert!(!plan.merged);
    g.ws.commit("Rename page", plan.ops).expect("commit");
    g.flush();
    assert_eq!(g.read("pages/Bar.md"), "- foo\n");
    assert!(!g.exists("pages/Foo.md"));
}

// ---- through the command queue -----------------------------------------------------------

#[test]
fn rename_runs_through_the_command_queue() {
    let g = graph(&[("pages/Old.md", OLD_PAGE), ("pages/Other.md", OTHER)]);
    let req = g.req("Old", "Fresh");
    let dir = g.dir;
    let (q, join) = CommandQueue::spawn(
        g.ws,
        Box::new(g.store),
        QueueConfig {
            auto_flush: true,
            debounce: None,
            ..QueueConfig::default()
        },
    );
    let report = q.rename_page(Source::Ui, req).expect("rename");
    assert_eq!(report.renamed, [("Old".to_owned(), "Fresh".to_owned())]);
    assert!(report.config_updated);
    assert_eq!(report.rewritten_pages.len(), 2);
    assert!(dir.path().join("pages/Fresh.md").exists());
    assert!(!dir.path().join("pages/Old.md").exists());
    let audit = q.audit();
    assert_eq!(audit.last().expect("audit").label, "Rename page");
    // The snapshot of the old key is gone, the new one is there.
    assert!(q.snapshot(&PageKey::from_title("Old")).is_none());
    assert!(q.snapshot(&PageKey::from_title("Fresh")).is_some());

    // Refused renames surface as QueueError::Rename.
    let mut again = RenameRequest {
        from: "Fresh".into(),
        to: "Other".into(),
        config: cfg(),
        lookup: None,
        merge: MergeMode::Refuse,
    };
    let err = q
        .rename_page(Source::Ui, again.clone())
        .expect_err("exists");
    assert!(matches!(
        err,
        QueueError::Rename(RenameError::TargetExists(_))
    ));
    again.merge = MergeMode::Merge {
        keep_aliases: false,
    };
    let merged = q.rename_page(Source::Ui, again).expect("merge");
    assert!(merged.merged);
    drop(join.shutdown());
    assert!(!dir.path().join("pages/Fresh.md").exists());
}
