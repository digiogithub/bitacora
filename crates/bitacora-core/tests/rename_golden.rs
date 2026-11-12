//! Rename cascade against a golden graph produced by Logseq 0.10.15 (BIT-T-0131,
//! BIT-SP-0002.R13).
//!
//! `fixtures/graphs/rename` is our own "before" graph; `fixtures/graphs/rename.expected` is what
//! Logseq 0.10.15 wrote after renaming `Old` to `New Idea` (see `PROVENANCE.md` there). The test runs
//! Bitacora's rename on a copy of the before graph and compares paths and lines. Logseq
//! re-serializes whole files (it drops the final newline), so files are compared line by line;
//! every line where Bitacora intentionally differs is listed in [`DIVERGENCES`].

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use bitacora_config::EffectiveConfig;
use bitacora_core::editor::{FsStore, MergeMode, PageFile, RefLookup, RenameRequest, Workspace};
use bitacora_core::graph_path::GraphPath;
use bitacora_core::naming::file_body_decode;

/// Index stand-in: every Markdown file under `pages/` and `journals/`.
struct DirLookup(std::path::PathBuf);

impl DirLookup {
    fn all(&self) -> Vec<(PageFile, String)> {
        let mut out = Vec::new();
        for dir in ["pages", "journals"] {
            let Ok(rd) = std::fs::read_dir(self.0.join(dir)) else {
                continue;
            };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let Some(body) = name.strip_suffix(".md") else {
                    continue;
                };
                let title = if dir == "journals" {
                    body.replace('_', "-")
                } else {
                    file_body_decode(body, bitacora_config::NameFormat::TripleLowbar)
                };
                out.push((
                    PageFile {
                        title,
                        path: GraphPath::new(&format!("{dir}/{name}")).expect("path"),
                    },
                    std::fs::read_to_string(e.path()).unwrap_or_default(),
                ));
            }
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

fn snapshot(root: &Path) -> BTreeMap<String, String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        for e in std::fs::read_dir(dir).expect("dir").flatten() {
            let p = e.path();
            let rel = p
                .strip_prefix(root)
                .expect("prefix")
                .to_string_lossy()
                .replace('\\', "/");
            if rel.starts_with("logseq/bak") || rel.ends_with("PROVENANCE.md") {
                continue;
            }
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                out.insert(rel, std::fs::read_to_string(&p).expect("read"));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn copy_tree(from: &Path, to: &Path) {
    for (rel, text) in snapshot(from) {
        let abs = to.join(&rel);
        std::fs::create_dir_all(abs.parent().expect("parent")).expect("mkdir");
        std::fs::write(abs, text).expect("copy");
    }
}

/// `(path, 1-based line, Bitacora, Logseq 0.10.15, reason)`.
type Divergence = (
    &'static str,
    usize,
    &'static str,
    &'static str,
    &'static str,
);

/// Every line where Bitacora's result differs from Logseq 0.10.15. The first four are choices;
/// the last one is a known gap with a backlog item, kept here so the test notices when it is
/// fixed.
const DIVERGENCES: &[Divergence] = &[
    (
        "logseq/config.edn",
        7,
        " :favorites [\"New Idea\" \"Other\"]",
        " :favorites [\"new idea\" \"Old\" \"Other\"]",
        "choice: the favourite is renamed in place with its case; Logseq adds a lower-cased \
         entry and leaves the stale one",
    ),
    (
        "pages/Front.md",
        1,
        "tags:: New Idea, x",
        "tags:: Old, x",
        "choice: tags:: values are page references, so they follow the rename; Logseq leaves \
         them pointing at a page that no longer has a file",
    ),
    (
        "pages/Refs.md",
        4,
        "- Lower-case link [[New Idea]] and tag #[[New Idea]].",
        "- Lower-case link [[old]] and tag #[[New Idea]].",
        "choice: page names are case-insensitive, so [[old]] is the same reference; Logseq \
         rewrites #old but not [[old]]",
    ),
    (
        "pages/Refs.md",
        11,
        "  tags:: New Idea, x",
        "  tags:: Old, x",
        "choice: same as the page-level tags:: above",
    ),
    (
        "pages/Refs.md",
        18,
        "- Embed {{embed [[New Idea]]}} and query {{query [[Old]]}}.",
        "- Embed {{embed [[New Idea]]}} and query {{query [[New Idea]]}}.",
        "gap (BIT-T-0518): references inside {{query ...}} are not rewritten yet",
    ),
];

/// `(Bitacora path, Logseq path, reason)`: files Bitacora leaves at another path.
const PATH_DIVERGENCES: &[(&str, &str, &str)] = &[(
    "pages/Notes on [[Old]].md",
    "pages/Notes on [[New Idea]].md",
    "gap (BIT-T-0517): a page whose title nests the renamed reference is not moved yet, while \
     links to it are rewritten",
)];

#[test]
fn rename_matches_the_logseq_golden_graph() {
    let before = bitacora_testkit::graph("rename");
    let expected = snapshot(&bitacora_testkit::graph("rename.expected"));
    let original = snapshot(&before);
    let dir = tempfile::tempdir().expect("tmp");
    copy_tree(&before, dir.path());

    let config = Arc::new(EffectiveConfig::from_texts(
        None,
        Some(&original["logseq/config.edn"]),
    ));
    let mut store = FsStore::new(dir.path());
    let mut ws = Workspace::new();
    let req = RenameRequest {
        from: "Old".into(),
        to: "New Idea".into(),
        config,
        lookup: Some(Arc::new(DirLookup(dir.path().to_path_buf()))),
        merge: MergeMode::Refuse,
    };
    let plan = ws.plan_rename(&store, &req).expect("plan");
    ws.commit("Rename page", plan.ops).expect("commit");
    let report = ws.flush(&mut store);
    assert!(report.is_complete(), "flush: {report:?}");
    let ours = snapshot(dir.path());

    // Same set of files at the same paths, except the listed path divergences.
    let mut found: Vec<(String, usize, String, String)> = Vec::new();
    let mut paired: Vec<(String, String)> = Vec::new();
    for path in expected.keys() {
        let mine = PATH_DIVERGENCES
            .iter()
            .find(|(_, theirs, _)| theirs == path)
            .map_or(path.as_str(), |(mine, _, _)| mine);
        assert!(
            ours.contains_key(mine),
            "{mine} is missing after the rename"
        );
        paired.push((mine.to_owned(), path.clone()));
    }
    assert_eq!(ours.len(), expected.len(), "{:?}", ours.keys());

    for (mine_path, path) in &paired {
        let (mine, theirs) = (&ours[mine_path], &expected[path]);
        let (a, b): (Vec<&str>, Vec<&str>) = (mine.lines().collect(), theirs.lines().collect());
        assert_eq!(a.len(), b.len(), "{path}: line count");
        for (i, (x, y)) in a.iter().zip(&b).enumerate() {
            if x != y {
                found.push((path.clone(), i + 1, (*x).to_owned(), (*y).to_owned()));
            }
        }
    }
    let allowed: Vec<(String, usize, String, String)> = DIVERGENCES
        .iter()
        .map(|(p, l, a, b, _)| ((*p).to_owned(), *l, (*a).to_owned(), (*b).to_owned()))
        .collect();
    assert_eq!(found, allowed, "divergences from Logseq 0.10.15");

    // User files are sacred: a line the rename did not touch keeps its bytes, and a file with
    // no touched line is byte-identical (Logseq itself drops the final newline of such files).
    for (path, mine) in &ours {
        let Some(orig) = original.get(path) else {
            continue;
        };
        if mine.lines().eq(orig.lines()) {
            assert_eq!(mine, orig, "{path} must be byte-identical");
        }
    }
    assert_eq!(ours["pages/Other.md"], original["pages/Other.md"]);
}
