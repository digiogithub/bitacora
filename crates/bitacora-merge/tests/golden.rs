//! Data-driven golden merge matrix (BIT-T-0370): every `fixtures/merge/<case>/` holds
//! `base.md`, `ours.md`, `theirs.md`, `expected.md` and optionally `conflicts.json` (a list of
//! `{kind, field, ours, theirs}`; absent means no conflicts). Run with `BITACORA_BLESS=1` to
//! (re)write the expectations, then review the diff by hand.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use bitacora_merge::{ConflictKind, MergeEnv, MergeResult, merge_page};
use serde_json::{Value, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/merge")
}

fn read(dir: &Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).unwrap_or_default()
}

fn kind(k: ConflictKind) -> &'static str {
    match k {
        ConflictKind::Content => "content",
        ConflictKind::Property => "property",
        ConflictKind::DeleteVsModify => "delete_vs_modify",
    }
}

fn conflicts_json(r: &MergeResult) -> Value {
    Value::Array(
        r.conflicts
            .iter()
            .map(|c| {
                json!({
                    "kind": kind(c.conflict.kind),
                    "field": c.conflict.field,
                    "ours": c.conflict.ours,
                    "theirs": c.conflict.theirs,
                })
            })
            .collect(),
    )
}

fn kinds(r: &MergeResult) -> Vec<(&'static str, String)> {
    let mut v: Vec<_> = r
        .conflicts
        .iter()
        .map(|c| (kind(c.conflict.kind), c.conflict.field.clone()))
        .collect();
    v.sort();
    v
}

fn cases() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(root())
        .expect("fixtures/merge exists")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    v.sort();
    v
}

#[test]
fn golden_matrix() {
    let bless = std::env::var_os("BITACORA_BLESS").is_some();
    let env = MergeEnv::new();
    let all = cases();
    assert!(
        all.len() >= 15,
        "expected the full matrix, found {}",
        all.len()
    );
    for dir in all {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (b, o, t) = (
            read(&dir, "base.md"),
            read(&dir, "ours.md"),
            read(&dir, "theirs.md"),
        );
        let r = merge_page(&b, &o, &t, &env);
        let got = conflicts_json(&r);
        if bless {
            fs::write(dir.join("expected.md"), &r.output).expect("write expected");
            let cj = dir.join("conflicts.json");
            if r.conflicts.is_empty() {
                let _ = fs::remove_file(cj);
            } else {
                fs::write(cj, serde_json::to_string_pretty(&got).expect("json") + "\n")
                    .expect("write conflicts");
            }
        }
        assert_eq!(r.output, read(&dir, "expected.md"), "{name}: output");
        let want: Value = match fs::read_to_string(dir.join("conflicts.json")) {
            Ok(s) => serde_json::from_str(&s).expect("valid conflicts.json"),
            Err(_) => json!([]),
        };
        assert_eq!(got, want, "{name}: conflicts");

        // Invariants for every case.
        assert!(
            !r.output.contains("<<<<<<<") && !r.output.contains(">>>>>>>"),
            "{name}: markers"
        );
        assert_eq!(merge_page(&b, &o, &t, &env), r, "{name}: deterministic");
        let again = merge_page(&b, &r.output, &t, &env);
        assert!(
            !again.output.contains("<<<<<<<"),
            "{name}: re-merge markers"
        );
        let sw = merge_page(&b, &t, &o, &env);
        assert_eq!(kinds(&r), kinds(&sw), "{name}: symmetric conflict set");
    }
}
