//! Golden tests of `parse` over `fixtures/graphs/**` (BIT-US-0005 / BIT-T-0027).
//!
//! Snapshots (insta) render the refs, properties, task columns and intervals of every file in a
//! compact text form; review changes with `cargo insta review`.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use std::fmt::Write as _;
use std::path::PathBuf;

use bitacora_config::EffectiveConfig;
use bitacora_core::scan::scan_graph;
use bitacora_index::{ParseConfig, ParsedFile, parse};

fn graph_root(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/graphs")
        .join(name)
}

fn parse_graph(name: &str) -> Vec<(String, ParsedFile)> {
    let root = graph_root(name);
    let cfg = EffectiveConfig::load(&root, None);
    let mut files = scan_graph(&root, &cfg).expect("scan");
    files.sort_by(|a, b| a.path.as_str().cmp(b.path.as_str()));
    files
        .into_iter()
        .filter(|f| matches!(f.path.extension(), Some("md" | "org")))
        .map(|f| {
            let bytes = std::fs::read(&f.abs).expect("read");
            let parsed = parse(&f.path, &bytes, &ParseConfig::new(&cfg));
            (f.path.as_str().to_owned(), parsed)
        })
        .collect()
}

fn check_invariants(path: &str, f: &ParsedFile) {
    for (i, b) in f.blocks.iter().enumerate() {
        assert_eq!(b.ord as usize, i, "{path}: ord");
        assert!(
            b.subtree_end >= b.ord && (b.subtree_end as usize) < f.blocks.len(),
            "{path}"
        );
        if let Some(p) = b.parent_ord {
            let parent = &f.blocks[p as usize];
            assert!(
                p < b.ord && parent.subtree_end >= b.subtree_end,
                "{path}: nesting"
            );
            assert_eq!(parent.depth + 1, b.depth, "{path}: depth");
        } else {
            assert_eq!(b.depth, 1, "{path}: top depth");
        }
        assert!(b.byte_start <= b.byte_end, "{path}: span");
        // Siblings are numbered 0.. in order.
        let sibs: Vec<_> = f
            .blocks
            .iter()
            .filter(|c| c.parent_ord == b.parent_ord)
            .map(|c| c.sibling_idx)
            .collect();
        assert_eq!(
            sibs,
            (0..sibs.len() as u32).collect::<Vec<_>>(),
            "{path}: sibling_idx"
        );
    }
}

fn render(path: &str, f: &ParsedFile, out: &mut String) {
    writeln!(
        out,
        "== {path} :: {} ({}){}",
        f.page.name,
        f.page.original_name,
        f.page
            .journal_day
            .map_or(String::new(), |d| format!(" journal={d}"))
    )
    .unwrap();
    if !f.page.aliases.is_empty() {
        let v: Vec<_> = f.page.aliases.iter().map(|p| p.name.as_str()).collect();
        writeln!(out, "  aliases: {v:?}").unwrap();
    }
    if !f.page.tags.is_empty() {
        let v: Vec<_> = f.page.tags.iter().map(|p| p.name.as_str()).collect();
        writeln!(out, "  tags: {v:?}").unwrap();
    }
    for d in &f.diagnostics {
        writeln!(
            out,
            "  diag {} L{:?}: {}",
            d.kind.as_str(),
            d.line,
            d.message
        )
        .unwrap();
    }
    for b in &f.blocks {
        write!(
            out,
            "  [{}..{}] d{} s{} p{:?}{}",
            b.ord,
            b.subtree_end,
            b.depth,
            b.sibling_idx,
            b.parent_ord,
            if b.is_pre_block { " PRE" } else { "" }
        )
        .unwrap();
        if let Some(m) = &b.marker {
            write!(out, " {m}").unwrap();
        }
        if let Some(p) = &b.priority {
            write!(out, " [#{p}]").unwrap();
        }
        if let Some(s) = &b.scheduled_raw {
            write!(out, " S={}", s).unwrap();
        }
        if let Some(s) = &b.deadline_raw {
            write!(out, " D={s}").unwrap();
        }
        if b.repeated {
            out.push_str(" repeated");
        }
        if b.collapsed {
            out.push_str(" collapsed");
        }
        if let Some(h) = b.heading {
            write!(out, " h{h}").unwrap();
        }
        if let Some(u) = &b.explicit_uuid {
            write!(out, " id={u}").unwrap();
        }
        writeln!(out, " | {:?}", b.title).unwrap();
        if !b.page_refs.is_empty() {
            let v: Vec<_> = b
                .page_refs
                .iter()
                .map(|r| format!("{}:{}", r.kind as u8, r.page.name))
                .collect();
            writeln!(out, "      pages: {}", v.join(" ")).unwrap();
        }
        if !b.block_refs.is_empty() {
            let v: Vec<_> = b
                .block_refs
                .iter()
                .map(|r| format!("{}:{}", r.kind as u8, r.target_uuid))
                .collect();
            writeln!(out, "      blocks: {}", v.join(" ")).unwrap();
        }
        for p in &b.properties {
            let vals: Vec<_> = p
                .values
                .iter()
                .map(|v| match v.value_num {
                    Some(n) => format!("{}={n}", v.value_norm),
                    None => v.value_norm.clone(),
                })
                .collect();
            writeln!(
                out,
                "      prop {} t{} b{} {:?} -> {vals:?}",
                p.key, p.value_type as u8, p.builtin, p.raw_value
            )
            .unwrap();
        }
    }
}

#[test]
fn edge_cases_graph() {
    let files = parse_graph("edge-cases");
    assert!(files.len() > 30);
    let mut out = String::new();
    for (path, f) in &files {
        check_invariants(path, f);
        render(path, f, &mut out);
    }
    insta::assert_snapshot!("edge_cases", out);
}

#[test]
fn legacy_names_graph() {
    let files = parse_graph("edge-cases-legacy-names");
    let mut out = String::new();
    for (path, f) in &files {
        check_invariants(path, f);
        render(path, f, &mut out);
    }
    insta::assert_snapshot!("legacy_names", out);
}

/// The docs graph is large: snapshot per-file aggregates, check the invariants everywhere.
#[test]
fn logseq_docs_graph() {
    let files = parse_graph("logseq-docs");
    assert!(files.len() > 100);
    let mut out = String::new();
    for (path, f) in &files {
        check_invariants(path, f);
        let pages: usize = f.blocks.iter().map(|b| b.page_refs.len()).sum();
        let blocks: usize = f.blocks.iter().map(|b| b.block_refs.len()).sum();
        let props: usize = f.blocks.iter().map(|b| b.properties.len()).sum();
        let tasks = f.blocks.iter().filter(|b| b.marker.is_some()).count();
        writeln!(
            out,
            "{path} :: {} blocks={} pagerefs={pages} blockrefs={blocks} props={props} tasks={tasks} referenced={} diags={}",
            f.page.name,
            f.blocks.len(),
            f.referenced_pages.len(),
            f.diagnostics.len()
        )
        .unwrap();
    }
    insta::assert_snapshot!("logseq_docs_summary", out);
}
