//! Behaviour tests for `parse` (BIT-US-0005): intervals, refs, properties, tasks.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use bitacora_config::EffectiveConfig;
use bitacora_core::graph_path::GraphPath;
use bitacora_index::{
    BlockRefKind, DiagnosticKind, FileFormat, PageRefKind, ParseConfig, ParsedFile, ValueType,
    parse,
};

const U1: &str = "6500c1a4-0000-4000-8000-000000000001";

fn run_cfg(path: &str, text: &str, cfg_src: Option<&str>) -> ParsedFile {
    let cfg = EffectiveConfig::from_texts(None, cfg_src);
    parse(
        &GraphPath::new(path).unwrap(),
        text.as_bytes(),
        &ParseConfig::new(&cfg),
    )
}

fn run(text: &str) -> ParsedFile {
    run_cfg("pages/p.md", text, None)
}

fn kinds(f: &ParsedFile, ord: usize) -> Vec<(String, PageRefKind)> {
    f.blocks[ord]
        .page_refs
        .iter()
        .map(|r| (r.page.name.clone(), r.kind))
        .collect()
}

#[test]
fn intervals_are_pre_order() {
    let f = run("- a\n  - b\n    - c\n  - d\n- e\n");
    let t: Vec<_> = f
        .blocks
        .iter()
        .map(|b| (b.ord, b.subtree_end, b.depth, b.sibling_idx, b.parent_ord))
        .collect();
    assert_eq!(
        t,
        [
            (0, 3, 1, 0, None),
            (1, 2, 2, 0, Some(0)),
            (2, 2, 3, 0, Some(1)),
            (3, 3, 2, 1, Some(0)),
            (4, 4, 1, 1, None),
        ]
    );
    assert_eq!(f.ancestors(2), [1, 0]);
}

#[test]
fn pre_block_is_ord_zero_and_blocks_follow() {
    let f = run("title:: My Page\ntags:: [[x]]\n\n- a\n  - b\n");
    assert_eq!(f.page.original_name, "My Page");
    assert_eq!(f.page.tags[0].name, "x");
    assert!(f.blocks[0].is_pre_block);
    assert_eq!(f.blocks[0].subtree_end, 0);
    assert_eq!(f.blocks[1].sibling_idx, 1);
    assert_eq!(f.blocks[1].subtree_end, 2);
    assert_eq!(f.blocks[2].parent_ord, Some(1));
}

#[test]
fn ref_kinds_follow_logseq() {
    let f = run(
        "- TODO [#A] see [[Link]] #tag #[[Multi Word]] {{embed [[Emb]]}} [[ns/child/leaf]]\n  k:: [[Prop Val]]\n",
    );
    let k = kinds(&f, 0);
    let has = |n: &str, kind| k.contains(&(n.to_owned(), kind));
    assert!(has("todo", PageRefKind::Marker));
    assert!(has("a", PageRefKind::Priority));
    assert!(has("link", PageRefKind::Link));
    assert!(has("tag", PageRefKind::Tag));
    assert!(has("multi word", PageRefKind::Tag));
    assert!(has("emb", PageRefKind::Embed));
    assert!(has("prop val", PageRefKind::PropertyValue));
    assert!(has("k", PageRefKind::PropertyName));
    assert!(has("ns", PageRefKind::NamespaceParent));
    assert!(has("ns/child", PageRefKind::NamespaceParent));
    assert!(has("ns/child/leaf", PageRefKind::Link));
    // Marker and priority pages are also upserted.
    let names: Vec<_> = f.referenced_pages.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"todo") && names.contains(&"a") && names.contains(&"ns"));
}

#[test]
fn property_names_respect_config_and_builtins() {
    let f =
        run("- x\n  id:: 6500c1a4-0000-4000-8000-000000000001\n  collapsed:: true\n  mine:: 1\n");
    let names: Vec<_> = kinds(&f, 0)
        .into_iter()
        .filter(|(_, k)| *k == PageRefKind::PropertyName)
        .map(|(n, _)| n)
        .collect();
    assert_eq!(names, ["mine"]);

    let f = run_cfg(
        "pages/p.md",
        "- x\n  mine:: 1\n",
        Some("{:property-pages/enabled? false}"),
    );
    assert!(kinds(&f, 0).is_empty());
    let f = run_cfg(
        "pages/p.md",
        "- x\n  mine:: 1\n  other:: 2\n",
        Some("{:property-pages/excludelist [:mine]}"),
    );
    assert_eq!(
        kinds(&f, 0),
        [("other".to_owned(), PageRefKind::PropertyName)]
    );
}

#[test]
fn block_ref_kinds() {
    let f = run(&format!(
        "- a (({U1})) {{{{embed (({U1}))}}}}\n- b [x]((({U1})))\n- c (({U1}))\n"
    ));
    let kinds =
        |o: usize| -> Vec<BlockRefKind> { f.blocks[o].block_refs.iter().map(|r| r.kind).collect() };
    assert_eq!(kinds(0), [BlockRefKind::Embed, BlockRefKind::Ref]);
    assert_eq!(kinds(1), [BlockRefKind::Link]);
    assert_eq!(kinds(2), [BlockRefKind::Ref]);
    assert_eq!(f.blocks[2].block_refs[0].target_uuid, U1);
}

#[test]
fn typed_properties() {
    let f = run(&format!(
        "- x\n  n:: 42\n  b:: true\n  s:: hello World\n  q:: \"quoted\"\n  r:: [[A]], [[b c]]\n  tags:: one, two\n  id:: {U1}\n  n:: 7\n"
    ));
    let p = &f.blocks[0].properties;
    let get = |k: &str| p.iter().find(|x| x.key == k).unwrap();
    assert_eq!(get("n").value_type, ValueType::Integer);
    assert_eq!(get("n").values[0].value_num, Some(7.0)); // last wins
    assert_eq!(p.iter().filter(|x| x.key == "n").count(), 1);
    assert_eq!(get("b").value_type, ValueType::Boolean);
    assert_eq!(get("b").values[0].value_num, Some(1.0));
    assert_eq!(get("s").value_type, ValueType::String);
    assert_eq!(get("s").values[0].value_norm, "hello world");
    assert_eq!(get("q").raw_value, "\"quoted\"");
    assert_eq!(get("r").value_type, ValueType::Refs);
    assert_eq!(get("r").values.len(), 2);
    assert_eq!(get("r").values[1].ref_page.as_ref().unwrap().name, "b c");
    assert_eq!(get("tags").value_type, ValueType::Refs);
    assert_eq!(get("tags").builtin, 1);
    assert_eq!(get("id").builtin, 2);
    assert_eq!(f.blocks[0].explicit_uuid.as_deref(), Some(U1));
    // pos is dense.
    let pos: Vec<_> = p.iter().map(|x| x.pos).collect();
    assert_eq!(pos, (0..p.len() as u32).collect::<Vec<_>>());
}

#[test]
fn comma_separated_keys_from_config() {
    let f = run_cfg(
        "pages/p.md",
        "- x\n  authors:: a, b\n  other:: c, d\n",
        Some("{:property/separated-by-commas #{:authors}}"),
    );
    let p = &f.blocks[0].properties;
    assert_eq!(
        p.iter().find(|x| x.key == "authors").unwrap().values.len(),
        2
    );
    assert_eq!(p.iter().find(|x| x.key == "other").unwrap().values.len(), 1);
}

#[test]
fn task_columns() {
    let f = run(
        "- DOING [#B] write it\n  SCHEDULED: <2024-03-04 Mon 10:30 .+1w>\n  DEADLINE: <2024-03-10 Sun>\n  collapsed:: true\n",
    );
    let b = &f.blocks[0];
    assert_eq!(b.marker.as_deref(), Some("DOING"));
    assert_eq!(b.priority.as_deref(), Some("B"));
    assert_eq!(b.title, "write it");
    assert_eq!(b.scheduled, Some(20_240_304));
    assert_eq!(
        b.scheduled_raw.as_deref(),
        Some("<2024-03-04 Mon 10:30 .+1w>")
    );
    assert_eq!(b.deadline, Some(20_240_310));
    assert_eq!(b.deadline_raw.as_deref(), Some("<2024-03-10 Sun>"));
    assert!(b.repeated);
    assert!(b.collapsed);
}

#[test]
fn heading_and_timestamps() {
    let f = run("- ## Big\n- x\n  heading:: 3\n  created-at:: 1700000000000\n");
    assert_eq!(f.blocks[0].heading, Some(2));
    assert_eq!(f.blocks[0].title, "Big");
    assert_eq!(f.blocks[1].heading, Some(3));
    assert_eq!(f.blocks[1].created_at, Some(1_700_000_000_000));
}

#[test]
fn search_text_strips_built_ins_and_folds() {
    let f = run(&format!("- Café ÉCOLE\n  id:: {U1}\n  mine:: Ñandú\n"));
    assert_eq!(f.blocks[0].search_text, "cafe ecole\nmine:: nandu");
    assert!(f.blocks[0].content.contains("id::"));
}

#[test]
fn path_refs_include_ancestors_and_page() {
    let f = run("- [[a]]\n  - [[b]]\n    - x\n- [[c]]\n");
    let p: Vec<_> = f.path_refs(2).into_iter().collect();
    assert_eq!(p, ["a", "b", "p"]);
    assert!(f.path_refs(3).contains("c") && !f.path_refs(3).contains("a"));
}

#[test]
fn journal_and_namespace_pages() {
    let f = run_cfg("journals/2024_03_04.md", "- x\n", None);
    assert_eq!(f.page.journal_day, Some(20_240_304));
    assert_eq!(f.page.original_name, "Mar 4th, 2024");
    let f = run_cfg(
        "pages/a___b___c.md",
        "- x\n",
        Some("{:file/name-format :triple-lowbar}"),
    );
    assert_eq!(f.page.name, "a/b/c");
    let ns: Vec<_> = f
        .page
        .namespace_parents
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(ns, ["a", "a/b"]);
    assert!(f.referenced_pages.iter().any(|p| p.name == "a/b"));
}

#[test]
fn diagnostics() {
    let f = run(
        "- a\n  id:: nope\n- b\n  id:: 6500c1a4-0000-4000-8000-000000000001\n- c\n  id:: 6500c1a4-0000-4000-8000-000000000001\n",
    );
    let k: Vec<_> = f.diagnostics.iter().map(|d| d.kind).collect();
    assert!(k.contains(&DiagnosticKind::InvalidProperty));
    assert!(k.contains(&DiagnosticKind::DuplicateBlockId));
}

#[test]
fn byte_spans_account_for_bom_and_crlf() {
    let src = "\u{feff}- a\r\n- b\r\n";
    let f = run(src);
    assert_eq!(f.blocks[0].byte_start, 3);
    assert_eq!(
        &src.as_bytes()[f.blocks[1].byte_start as usize..],
        b"- b\r\n"
    );
    assert_eq!(f.blocks[1].line_start, 2);
    assert_eq!(f.blocks[1].content, "b");
}

#[test]
fn org_and_binary_files_do_not_panic() {
    let f = run_cfg("pages/o.org", "* heading\n", None);
    assert_eq!(f.page.format, FileFormat::Org);
    assert!(f.blocks.is_empty());
    let cfg = EffectiveConfig::default();
    let f = parse(
        &GraphPath::new("pages/bad.md").unwrap(),
        b"- a\xff\xfe\n",
        &ParseConfig::new(&cfg),
    );
    assert!(f.blocks.is_empty());
    assert!(
        f.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::ParseError)
    );
}

#[test]
fn deterministic() {
    let a = run("- [[x]] #y\n  k:: v\n");
    let b = run("- [[x]] #y\n  k:: v\n");
    assert_eq!(a, b);
}
