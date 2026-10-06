//! Unified round-trip and golden suite (BIT-US-0095).
//!
//! * every `*.md` of `fixtures/graphs/**` and `fixtures/markdown/**` survives `parse -> serialize`
//!   byte for byte, through all three layers (outline spans, [`Document`], full `analyze`);
//! * the "round-trip fixtures to write ourselves" list of
//!   `docs/analysis/logseq/02-markdown-block-syntax.md` §11 (items 1-19) as in-memory cases;
//! * mutated corpus files (truncated, lines dropped/duplicated, EOL-converted) still round-trip;
//! * golden canonical serialization of edited blocks (CRLF, tabs, BOM, depth changes, inserts).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use bitacora_markdown::block::analyze;
use bitacora_markdown::properties::PropertyConfig;
use bitacora_markdown::{
    Document, ParserOptions, WriteOptions, content_of, pre_block_content, serialize, split,
};
use proptest::prelude::*;

fn md_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            md_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "md") {
            out.push(p);
        }
    }
}

fn corpus() -> Vec<(PathBuf, Vec<u8>)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let mut files = Vec::new();
    md_files(&root.join("graphs"), &mut files);
    md_files(&root.join("markdown"), &mut files);
    files
        .into_iter()
        .map(|f| {
            let b = std::fs::read(&f).expect("read fixture");
            (f, b)
        })
        .collect()
}

fn ser(doc: &Document) -> Vec<u8> {
    serialize(doc, &WriteOptions::default())
}

/// Asserts all layers reproduce `bytes` and that analysing every block never panics.
fn assert_lossless(name: &str, bytes: &[u8]) {
    // Layer 1: the outline spans partition the input.
    let outline = split(bytes);
    let rebuilt: Vec<u8> = outline
        .spans()
        .flat_map(|s| s.slice(bytes).iter().copied())
        .collect();
    assert_eq!(rebuilt, bytes, "{name}: outline spans");

    // Layer 2: the document model.
    let doc = Document::parse(bytes.to_vec());
    assert!(doc.is_clean(), "{name}: fresh document must be clean");
    assert_eq!(ser(&doc), bytes, "{name}: Document round trip");

    // Layer 3: analysing blocks is read-only and total.
    let cfg = PropertyConfig::default();
    if let Some(span) = outline.pre_block {
        let _ = analyze(
            &pre_block_content(bytes, span),
            &cfg,
            ParserOptions::default(),
        );
    }
    for b in &outline.blocks {
        let _ = analyze(&content_of(bytes, b), &cfg, ParserOptions::default());
    }
}

#[test]
fn every_fixture_round_trips_through_every_layer() {
    let files = corpus();
    assert!(files.len() > 60, "corpus too small: {}", files.len());
    for (f, bytes) in &files {
        assert_lossless(&f.display().to_string(), bytes);
    }
}

/// §11 "Round-trip fixtures to write ourselves", items 1-19 (plus CRLF/BOM/tabs from item 3).
const SPEC_CASES: &[(&str, &str)] = &[
    ("1 two-space indent", "- a\n  - b\n    - c\n  - d\n- e\n"),
    ("3 crlf", "- a\r\n  - b\r\n- c\r\n"),
    ("3 bom", "\u{feff}- a\n- b\n"),
    ("3 tabs and spaces", "- a\n\t- b\n  \t- c\n    - d\n"),
    ("4 blank lines", "- a\n\n- b\n  line\n\n  more\n\n\n- c\n"),
    ("5 front matter", "---\ntitle: T\ntags: a, b\n---\n\n- x\n"),
    ("5 title::", "title:: My page\nalias:: x\n\n- a\n"),
    ("5 #+title in block", "- a\n  #+title: not page-level\n"),
    (
        "6 fenced code",
        "- ```md\n  - x\n  foo:: bar\n  [[x]] ((6500c1a4-0000-4000-8000-000000000001))\n  ```\n- after\n",
    ),
    ("7 unclosed fence", "- ```\n  code\n- looks like a bullet\n"),
    (
        "8 property edge keys",
        "- a\n  a.b.c:: 1\n  empty::\n  my key:: v\n  key::value\n  q:: \"quoted\"\n  tags:: a, [[b c]], #d\n",
    ),
    (
        "9 properties drawer",
        "- a\n  :PROPERTIES:\n  :custom_id: x\n  :END:\n- b\n",
    ),
    (
        "10 markers",
        "- LATER\n- TODO [#A]\n- ## TODO h\n- DONE text\n",
    ),
    (
        "11 repeaters",
        "- t\n  SCHEDULED: <2024-01-01 Mon +1w>\n  DEADLINE: <2024-01-02 Tue ++1d>\n- u\n  SCHEDULED: <2024-01-01 Mon .+1m>\n- v [2024-01-01 Mon]\n",
    ),
    (
        "12 logbook",
        "- DOING a\n  :LOGBOOK:\n  CLOCK: [2024-01-01 Mon 10:00:00]\n  CLOCK: [2024-01-01 Mon 08:00:00]--[2024-01-01 Mon 08:30:00] =>  00:30:00\n  :END:\n",
    ),
    (
        "13 all ref forms",
        "- [[p]] #t #[[a b]] ((6500c1a4-0000-4000-8000-000000000001)) #[[nested [[tag]]]] [l]([[p]]) [[a/b/c]]\n",
    ),
    (
        "14 embeds and macros",
        "- {{embed [[p]]}} {{embed ((6500c1a4-0000-4000-8000-000000000001))}} {{query (todo)}} {{video https://x.y}} {{m 1 2}}\n",
    ),
    (
        "15 query block",
        "- #+BEGIN_QUERY\n  {:query [:find ?b :where [?b :block/page [[x]]]] :title \"q\"}\n  #+END_QUERY\n",
    ),
    ("16 first block heading", "- # Title\n- k:: v\n"),
    ("16 first block props", "- k:: v\n- b\n"),
    ("16 heading:: true", "- heading:: true\n  text\n"),
    ("17 collapsed", "- a\n  collapsed:: true\n  - b\n"),
    (
        "18 duplicate ids",
        "- a\n  id:: 6500c1a4-0000-4000-8000-000000000001\n- b\n  id:: 6500c1a4-0000-4000-8000-000000000001\n",
    ),
    (
        "19 image meta",
        "- ![x](../assets/a.png){:height 100, :width 200}\n",
    ),
    ("no trailing newline", "- a\n- b"),
    ("empty", ""),
    ("only newline", "\n"),
    ("text without bullets", "just text\nmore\n"),
];

#[test]
fn spec_round_trip_cases() {
    for (name, text) in SPEC_CASES {
        assert_lossless(name, text.as_bytes());
    }
}

fn arb_mutation() -> impl Strategy<Value = (usize, u8, usize)> {
    (0usize..400, 0u8..4, 0usize..4000)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Mutating real corpus files (cut anywhere, drop or duplicate a line, convert EOLs) keeps the
    /// byte round trip.
    #[test]
    fn mutated_corpus_files_round_trip((pick, kind, at) in arb_mutation()) {
        let files = corpus();
        let (_, bytes) = &files[pick % files.len()];
        let mutated: Vec<u8> = match kind {
            0 => bytes[..at.min(bytes.len())].to_vec(),
            1 | 2 => {
                let lines: Vec<&[u8]> = bytes.split_inclusive(|&b| b == b'\n').collect();
                if lines.is_empty() {
                    Vec::new()
                } else {
                    let k = at % lines.len();
                    let mut out = Vec::new();
                    for (i, l) in lines.iter().enumerate() {
                        if i == k && kind == 1 {
                            continue;
                        }
                        out.extend_from_slice(l);
                        if i == k {
                            out.extend_from_slice(l);
                        }
                    }
                    out
                }
            }
            _ => String::from_utf8_lossy(bytes).replace('\n', "\r\n").into_bytes(),
        };
        assert_lossless("mutated", &mutated);
    }
}

// ---------------------------------------------------------------------------------------------
// Golden canonical serialization of edited blocks. Expected strings are written by hand from
// `02-markdown-block-syntax.md` §7, never produced by the code under test.
// ---------------------------------------------------------------------------------------------

fn edited(src: &str, f: impl FnOnce(&mut Document)) -> String {
    let mut doc = Document::parse(src.as_bytes().to_vec());
    f(&mut doc);
    String::from_utf8(ser(&doc)).expect("utf8")
}

#[test]
fn golden_edit_keeps_untouched_blocks_verbatim() {
    let src = "- a\n    - odd indent\n- b\n";
    let out = edited(src, |d| {
        d.set_block_content(2, "b2");
    });
    assert_eq!(out, "- a\n    - odd indent\n- b2\n");
}

#[test]
fn golden_edit_in_crlf_file_writes_crlf() {
    let out = edited("- a\r\n- b\r\n", |d| {
        d.set_block_content(0, "a\nmore");
    });
    assert_eq!(out, "- a\r\n  more\r\n- b\r\n");
}

#[test]
fn golden_edit_in_tab_file_uses_tabs_and_keeps_bom() {
    let out = edited("\u{feff}title:: x\n\n- a\n\t- b\n", |d| {
        d.set_block_content(1, "b\nbody");
    });
    assert_eq!(out, "\u{feff}title:: x\n\n- a\n\t- b\n\t  body\n");
}

#[test]
fn golden_insert_and_depth_change() {
    let out = edited("- a\n- b\n", |d| {
        // Edited-node depth is 1-based: 1 is a top-level block.
        d.insert_block(1, 2, "child\nline2");
        d.set_depth(2, 3);
    });
    assert_eq!(out, "- a\n\t- child\n\t  line2\n\t\t- b\n");
}

#[test]
fn golden_last_block_without_eol_keeps_convention() {
    let out = edited("- a\n- b", |d| {
        d.set_block_content(1, "c");
    });
    assert_eq!(out, "- a\n- c");
}

#[test]
fn golden_remove_block_touches_nothing_else() {
    let out = edited("- a\n  - b\n- c\n", |d| {
        d.remove_block(1);
    });
    assert_eq!(out, "- a\n- c\n");
}
