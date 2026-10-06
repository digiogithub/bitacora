//! Outline fixtures: `fixtures/markdown/outline/*.md` with `*.expected.json` produced from mldoc
//! 1.5.7 (`tools/mldoc-diff`). Checks block starts, raw levels, parents and the byte partition.

use std::path::PathBuf;

use bitacora_markdown::{build_tree, split};
use serde_json::Value;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/markdown/outline")
}

fn load(name: &str) -> Vec<u8> {
    std::fs::read(fixture_dir().join(name)).unwrap_or_else(|e| panic!("read {name}: {e}"))
}

#[test]
fn outline_fixtures() {
    let mut checked = 0;
    let mut entries: Vec<_> = std::fs::read_dir(fixture_dir())
        .expect("fixture dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        let input = load(&name);
        let expected: Value = serde_json::from_slice(&load(&name.replace(".md", ".expected.json")))
            .unwrap_or_else(|e| panic!("{name}: bad expected json: {e}"));
        let expected = expected["blocks"].as_array().expect("blocks array");

        let outline = split(&input);
        assert_eq!(outline.blocks.len(), expected.len(), "{name}: block count");
        let links = build_tree(&outline.blocks);
        for (i, (b, e)) in outline.blocks.iter().zip(expected).enumerate() {
            assert_eq!(b.span.start as u64, e["start"], "{name}: block {i} start");
            assert_eq!(b.raw_level as u64, e["level"], "{name}: block {i} level");
            let parent = links[i]
                .parent
                .map_or(Value::Null, |p| Value::from(p as u64));
            assert_eq!(parent, e["parent"], "{name}: block {i} parent");
        }

        // Pre-block: everything before the first block, or the whole file without blocks.
        let first = expected
            .first()
            .map_or(input.len() as u64, |e| e["start"].as_u64().expect("start"));
        assert_eq!(
            outline.pre_block.map_or(0, |s| s.len() as u64),
            first,
            "{name}: pre-block"
        );

        // Lossless: the spans partition the input.
        let rebuilt: Vec<u8> = outline
            .spans()
            .flat_map(|s| s.slice(&input).iter().copied())
            .collect();
        assert_eq!(rebuilt, input, "{name}: round trip");
        checked += 1;
    }
    assert!(checked >= 14, "only {checked} fixtures found");
}

#[test]
fn crlf_fixture_really_has_crlf_and_bom_fixture_really_has_a_bom() {
    assert!(load("crlf.md").windows(2).any(|w| w == b"\r\n"));
    assert!(load("bom.md").starts_with(b"\xef\xbb\xbf"));
    assert!(load("bom-bullet.md").starts_with(b"\xef\xbb\xbf"));
}
