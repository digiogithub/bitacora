//! Graph-load golden tests over the committed fixture graphs (read-only).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use bitacora_config::EffectiveConfig;
use bitacora_core::graph::{Diagnostic, Graph, PageKey, PageOrigin};
use bitacora_core::graph_path::GraphPath;
use walkdir::WalkDir;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/graphs")
        .join(name)
}

fn load_with(name: &str, cfg_src: Option<&str>) -> Graph {
    let root = fixture(name);
    let cfg = match cfg_src {
        Some(s) => EffectiveConfig::from_texts(None, Some(s)),
        None => EffectiveConfig::load(&root, None),
    };
    Graph::load(&root, &cfg).expect("load")
}

fn gp(s: &str) -> GraphPath {
    GraphPath::new(s).expect("path")
}

fn key(s: &str) -> PageKey {
    PageKey::from_title(s)
}

fn snapshot(root: &Path) -> Vec<(String, u64, std::time::SystemTime)> {
    let mut v: Vec<_> = WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .map(|e| {
            let m = e.metadata().expect("meta");
            (
                e.path().display().to_string(),
                m.len(),
                m.modified().expect("mtime"),
            )
        })
        .collect();
    v.sort();
    v
}

#[test]
fn loading_fixture_graphs_writes_nothing() {
    for name in [
        "edge-cases",
        "edge-cases-legacy-names",
        "logseq-docs",
        "journals",
        "ignore-rules",
    ] {
        let root = fixture(name);
        let before = snapshot(&root);
        let g = load_with(name, None);
        assert!(!g.is_empty(), "{name}");
        assert_eq!(snapshot(&root), before, "{name}");
    }
}

#[test]
fn journals_fixture_default_config() {
    let g = load_with("journals", None);
    // Journals are detected by title whatever their file name style, even under pages/.
    for (title, file) in [
        ("Nov 14th, 2025", "journals/2025_11_14.md"),
        ("Nov 15th, 2025", "journals/2025-11-15.md"),
        ("Nov 16th, 2025", "journals/Nov 16th, 2025.md"),
        ("Nov 18th, 2025", "journals/2025_11_18.org"),
        ("Nov 19th, 2025", "pages/2025_11_19.md"),
    ] {
        let p = g.resolve(title).unwrap_or_else(|| panic!("{title}"));
        assert!(p.journal.is_some(), "{title}");
        assert_eq!(p.file.as_ref().map(|f| f.as_str()), Some(file), "{title}");
    }
    assert!(g.resolve("Nov 18th, 2025").expect("org").read_only);
    // Not recognised as journal titles: plain pages.
    assert!(g.resolve("20251117").expect("plain").journal.is_none());
    assert!(g.resolve("notes").expect("plain").journal.is_none());
    // Same date from `journals/` and `pages/`: first in filter-files order (journals) wins.
    assert_eq!(
        g.diagnostics(),
        [Diagnostic::DuplicateTitle {
            key: key("nov 14th, 2025"),
            kept: gp("journals/2025_11_14.md"),
            skipped: gp("pages/Nov 14th, 2025.md"),
        }]
    );
}

#[test]
fn journals_fixture_other_title_formats() {
    let g = load_with(
        "journals",
        Some(r#"{:journal/page-title-format "yyyy-MM-dd"}"#),
    );
    let p = g.resolve("2025-11-14").expect("journal");
    assert_eq!(p.journal.as_ref().map(|j| j.journal_day), Some(20251114));
    assert_eq!(p.original_name, "2025-11-14");
    // Titles in the default style still parse and are re-rendered in the configured format.
    assert_eq!(
        g.resolve("2025-11-16")
            .expect("journal")
            .file
            .as_ref()
            .map(|f| f.as_str()),
        Some("journals/Nov 16th, 2025.md")
    );
    let g = load_with(
        "journals",
        Some(r#"{:journal/page-title-format "yyyy_MM_dd"}"#),
    );
    assert!(g.resolve("2025_11_14").expect("journal").journal.is_some());
}

#[test]
fn ignore_rules_fixture_pages() {
    let g = load_with("ignore-rules", None);
    let files: Vec<&str> = g
        .pages()
        .filter_map(|p| p.file.as_ref().map(|f| f.as_str()))
        .collect();
    for ignored in [
        "logseq/bak/pages/a/2025.md",
        "logseq/.recycle/old.md",
        "pages/.hidden.md",
        "pages/UPPER.MD",
    ] {
        assert!(!files.contains(&ignored), "{ignored}");
    }
    assert!(files.contains(&"pages/b.markdown"));
    // `pages/sub/c.md` is a plain `c` page, not a namespace.
    assert!(g.resolve("c").is_some());
    let hidden = load_with("ignore-rules", Some(r#"{:hidden ["/archived" "test.md"]}"#));
    assert!(hidden.resolve("old").is_none());
    assert!(g.resolve("old").is_some());
}

#[test]
fn edge_cases_fixture_model() {
    let g = load_with("edge-cases", None);
    // Every file-backed page keeps its path; virtual pages never have one.
    for p in g.pages() {
        assert_eq!(p.is_virtual(), p.origin != PageOrigin::File);
    }
    // `aa?#/bbb/ccc` decodes from `aa%3F%23___bbb___ccc.md` and gets virtual namespace parents.
    let p = g.resolve("aa?#/bbb/ccc").expect("page");
    assert_eq!(p.namespace_parent, Some(key("aa?#/bbb")));
    assert!(g.page(&key("aa?#")).expect("ns").is_virtual());
}
