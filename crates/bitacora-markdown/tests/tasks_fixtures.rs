//! Differential test of block heads, planning lines and drawers against mldoc 1.5.7.
//!
//! `fixtures/markdown/tasks/*.md` are Markdown pages and `*.expected.json` what mldoc derives per
//! block (`node tools/mldoc-diff/tasks.js <file>`, one process per file). Every block must give the
//! same marker, priority, heading size, lifted SCHEDULED/DEADLINE and drawers.

use std::path::PathBuf;

use bitacora_markdown::block::analyze;
use bitacora_markdown::properties::PropertyConfig;
use bitacora_markdown::{ParserOptions, content_of, split};
use serde_json::{Value, json};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/markdown/tasks")
}

#[test]
fn task_fixtures_match_mldoc() {
    let mut names: Vec<_> = std::fs::read_dir(dir())
        .expect("fixture dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    names.sort();
    assert!(names.len() >= 3);
    for path in names {
        let name = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        let input = std::fs::read(&path).expect("read");
        let expected: Value = serde_json::from_slice(
            &std::fs::read(path.with_extension("expected.json")).expect("expected"),
        )
        .expect("json");
        let expected = expected["blocks"].as_array().expect("blocks");
        let outline = split(&input);
        assert_eq!(outline.blocks.len(), expected.len(), "{name}: block count");

        for (i, (block, want)) in outline.blocks.iter().zip(expected).enumerate() {
            let content = content_of(&input, block);
            let a = analyze(
                &content,
                &PropertyConfig::default(),
                ParserOptions::default(),
            );
            let ctx = format!("{name}: block {i} {content:?}");
            assert_eq!(
                a.head.marker.map(|m| m.as_str()),
                want["marker"].as_str(),
                "{ctx}: marker"
            );
            assert_eq!(
                a.head.priority.map(|c| c.to_string()),
                want["priority"].as_str().map(str::to_owned),
                "{ctx}: priority"
            );
            assert_eq!(
                a.head.heading.map(|h| h as u64),
                want["heading"].as_u64(),
                "{ctx}: heading size"
            );
            assert_eq!(
                a.scheduled.map(u64::from),
                want["scheduled"].as_u64(),
                "{ctx}: scheduled"
            );
            assert_eq!(
                a.deadline.map(u64::from),
                want["deadline"].as_u64(),
                "{ctx}: deadline"
            );
            assert_eq!(
                a.repeated,
                want["repeated"] == json!(true),
                "{ctx}: repeated"
            );

            let drawers: Vec<Value> = a
                .drawers
                .iter()
                .map(|d| {
                    let lines: Vec<&str> =
                        d.lines.iter().map(|l| content[l.range()].trim()).collect();
                    json!({"name": d.name.to_lowercase(), "lines": lines})
                })
                .collect();
            assert_eq!(Value::Array(drawers), want["drawers"], "{ctx}: drawers");
        }
    }
}
