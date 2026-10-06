//! Differential test of the inline scanner against mldoc 1.5.7.
//!
//! `fixtures/markdown/inline/cases.txt` holds one inline snippet per line and
//! `cases.expected.json` the tokens mldoc finds in each (regenerate with
//! `node tools/mldoc-diff/inline.js fixtures/markdown/inline/cases.txt`). Every case must produce
//! the same flattened token list from our scanner.

use std::path::PathBuf;

use bitacora_markdown::inline::{InlineToken, LinkTarget, PageRef, scan};
use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markdown/inline")
        .join(name)
}

fn nested(text: &str, refs: &[PageRef], out: &mut Vec<Value>) {
    for p in refs {
        out.push(json!(["page", &text[p.name.range()]]));
        nested(text, &p.nested, out);
    }
}

/// Flattens our tokens into the vocabulary of `tools/mldoc-diff/inline.js`.
fn flatten(text: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for t in scan(text) {
        match t {
            InlineToken::Code(s) => {
                let run = text[s.range()].bytes().take_while(|&b| b == b'`').count();
                out.push(json!(["code", &text[s.start + run..s.end - run]]));
            }
            InlineToken::Math(_) => out.push(json!(["math"])),
            InlineToken::Html(_) => out.push(json!(["html"])),
            InlineToken::Url(_) => out.push(json!(["url"])),
            InlineToken::PageRef(p) => {
                out.push(json!(["page", &text[p.name.range()]]));
                nested(text, &p.nested, &mut out);
            }
            InlineToken::Tag(t) => {
                out.push(json!(["tag", &text[t.name.range()]]));
                nested(text, &t.nested, &mut out);
            }
            InlineToken::BlockRef(b) => out.push(json!(["block", &text[b.id.range()]])),
            InlineToken::Link(l) => match l.target {
                LinkTarget::Page(p) => {
                    out.push(json!(["page", &text[p.name.range()]]));
                    nested(text, &p.nested, &mut out);
                }
                LinkTarget::Block(b) => out.push(json!(["block", &text[b.id.range()]])),
                LinkTarget::File(_) => out.push(json!(["file", &text[l.label.range()]])),
                LinkTarget::Url(_) => out.push(json!(["url"])),
                LinkTarget::Search(_) => out.push(json!(["search"])),
            },
            InlineToken::Macro(m) => {
                let mut v = vec![json!("macro"), json!(&text[m.name.range()])];
                v.extend(m.args.iter().map(|a| json!(&text[a.range()])));
                out.push(Value::Array(v));
            }
        }
    }
    out
}

#[test]
fn inline_scanner_matches_mldoc() {
    let expected: Value = serde_json::from_slice(
        &std::fs::read(fixture("cases.expected.json")).expect("expected json"),
    )
    .expect("valid json");
    let cases = expected.as_array().expect("array");
    let corpus = std::fs::read_to_string(fixture("cases.txt")).expect("cases");
    assert_eq!(
        corpus.lines().filter(|l| !l.is_empty()).count(),
        cases.len(),
        "cases.txt and cases.expected.json are out of sync"
    );

    let mut failures = Vec::new();
    for c in cases {
        let text = c["case"].as_str().expect("case");
        let want = c["tokens"].as_array().expect("tokens");
        let got = flatten(text);
        if &got != want {
            failures.push(format!("{text:?}\n   mldoc: {want:?}\n   ours:  {got:?}"));
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
