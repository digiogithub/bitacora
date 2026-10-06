//! Differential test of page properties against mldoc 1.5.7 + Logseq's hoisting rules.
//!
//! `fixtures/markdown/page-props/cases.txt` has pages separated by `====` lines and
//! `cases.expected.json` the page properties mldoc yields (`node tools/mldoc-diff/pageprops.js`).

use std::path::PathBuf;

use bitacora_markdown::ParserOptions;
use bitacora_markdown::page_props::page_properties;
use serde_json::{Value, json};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/markdown/page-props")
}

#[test]
fn page_properties_match_mldoc() {
    let expected: Value =
        serde_json::from_slice(&std::fs::read(dir().join("cases.expected.json")).expect("json"))
            .expect("valid json");
    let cases = expected.as_array().expect("array");
    let mut corpus = std::fs::read_to_string(dir().join("cases.txt")).expect("cases");
    if corpus.ends_with('\n') {
        corpus.pop();
    }
    let sources: Vec<&str> = corpus.split("\n====\n").collect();
    assert_eq!(
        sources.len(),
        cases.len(),
        "corpus and expectations differ in size"
    );

    let mut failures = Vec::new();
    for (src, case) in sources.iter().zip(cases) {
        assert_eq!(case["case"].as_str(), Some(*src));
        let got: Vec<Value> = page_properties(src.as_bytes(), ParserOptions::default())
            .properties
            .iter()
            .map(|p| json!([p.key_raw.to_lowercase(), p.value_raw]))
            .collect();
        if Value::Array(got.clone()) != case["properties"] {
            failures.push(format!(
                "{src:?}\n   mldoc: {}\n   ours:  {}",
                case["properties"],
                Value::Array(got)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}
