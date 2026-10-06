//! Byte-preserving serializer: `serialize(parse(bytes)) == bytes` over every fixture, minimal-diff
//! guarantees when one block is edited, and the canonical form of `02-markdown-block-syntax.md` §7.

use std::path::{Path, PathBuf};

use bitacora_markdown::{Document, Node, WriteOptions, serialize};
use bitacora_testkit::{graph_names, markdown_files};

fn md_files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            md_files_under(&p, out);
        } else if p.extension().is_some_and(|e| e == "md") {
            out.push(p);
        }
    }
}

/// Every `*.md` of `fixtures/graphs/**` (pages and journals) and `fixtures/markdown/**`.
fn all_fixture_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = graph_names()
        .iter()
        .flat_map(|g| markdown_files(g))
        .collect();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/markdown");
    md_files_under(&root, &mut files);
    files
}

fn ser(doc: &Document) -> Vec<u8> {
    serialize(doc, &WriteOptions::default())
}

#[test]
fn round_trip_is_byte_identical_for_every_fixture() {
    let files = all_fixture_files();
    assert!(
        files.len() > 60,
        "expected the fixture corpus, saw {}",
        files.len()
    );
    for f in files {
        let bytes = std::fs::read(&f).expect("read fixture");
        let doc = Document::parse(bytes.clone());
        assert!(doc.is_clean());
        assert_eq!(ser(&doc), bytes, "{}", f.display());
    }
}

/// Editing one block rewrites only that block: everything before and after stays byte-identical.
#[test]
fn editing_one_block_changes_only_its_range() {
    let mut edited = 0;
    for f in all_fixture_files() {
        let bytes = std::fs::read(&f).expect("read fixture");
        let base = Document::parse(bytes.clone());
        for i in 0..base.blocks.len() {
            let Node::Original { block, .. } = &base.blocks[i] else {
                continue;
            };
            let (start, end) = (block.span.start, block.span.end);
            let mut doc = base.clone();
            let content = doc.block_content(i).expect("content").into_owned();
            doc.set_block_content(i, &format!("{content}\nedited:: yes"));
            let out = ser(&doc);
            assert!(
                out.starts_with(&bytes[..start]),
                "{} block {i}: prefix changed",
                f.display()
            );
            assert!(
                out.ends_with(&bytes[end..]),
                "{} block {i}: suffix changed",
                f.display()
            );
            edited += 1;
        }
    }
    assert!(edited > 100, "only {edited} blocks edited");
}

#[test]
fn canonical_example_is_rebuilt_byte_for_byte() {
    // `02-markdown-block-syntax.md` §7 "Canonical form".
    let expected = "title:: Example\ntags:: demo\n\n- TODO [#A] Parent block #tag\n  SCHEDULED: <2024-01-01 Mon .+1d>\n  id:: 6500c1a4-0000-4000-8000-000000000001\n  :LOGBOOK:\n  CLOCK: [2024-01-01 Mon 10:00:00]--[2024-01-01 Mon 11:00:00] =>  01:00:00\n  :END:\n\t- child with ((6500c1a4-0000-4000-8000-000000000001))\n\t  second line of child\n\t  ```clojure\n\t  (+ 1 2)\n\t  ```\n\t-\n- collapsed parent\n  collapsed:: true\n\t- hidden child";
    let mut d = Document::parse("");
    d.set_pre_block_content("title:: Example\ntags:: demo");
    let blocks: [(usize, &str); 5] = [
        (
            1,
            "TODO [#A] Parent block #tag\nSCHEDULED: <2024-01-01 Mon .+1d>\nid:: 6500c1a4-0000-4000-8000-000000000001\n:LOGBOOK:\nCLOCK: [2024-01-01 Mon 10:00:00]--[2024-01-01 Mon 11:00:00] =>  01:00:00\n:END:",
        ),
        (
            2,
            "child with ((6500c1a4-0000-4000-8000-000000000001))\nsecond line of child\n```clojure\n(+ 1 2)\n```",
        ),
        (2, ""),
        (1, "collapsed parent\ncollapsed:: true"),
        (2, "hidden child"),
    ];
    for (i, (depth, content)) in blocks.iter().enumerate() {
        d.insert_block(i, *depth, content);
    }
    assert_eq!(String::from_utf8(ser(&d)).expect("utf8"), expected);

    // The doc's own canonical text is a fixed point: parse, no edit -> identical, and re-reading
    // every block's content gives back what we put in.
    let parsed = Document::parse(expected);
    assert_eq!(ser(&parsed), expected.as_bytes());
    for (i, (_, content)) in blocks.iter().enumerate() {
        assert_eq!(
            parsed.block_content(i).expect("block"),
            *content,
            "block {i}"
        );
    }
}

#[test]
fn spec_examples() {
    // New page: tags + an empty block.
    let mut d = Document::parse("");
    d.set_pre_block_content("tags:: demo");
    d.insert_block(0, 1, "");
    assert_eq!(ser(&d), b"tags:: demo\n\n-");
    // Two-space unit, depth 3.
    let mut d = Document::parse("- a\n  - b\n");
    d.insert_block(2, 3, "x");
    assert_eq!(ser(&d), b"- a\n  - b\n    - x\n");
    // Edited first block with `heading:: true` keeps its bullet.
    let mut d = Document::parse("- Intro\n  heading:: true\n- b\n");
    d.set_block_content(0, "Intro\nheading:: true\nmore");
    assert_eq!(ser(&d), b"- Intro\n  heading:: true\n  more\n- b\n");
    // Appending after a file without trailing newline.
    let mut d = Document::parse("- a\n- last");
    d.insert_block(2, 1, "new");
    assert_eq!(ser(&d), b"- a\n- last\n- new");
}
