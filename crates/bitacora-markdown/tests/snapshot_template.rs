//! Template: `insta` snapshot tests over the edge-case fixture graph.
//!
//! Workflow: run `cargo insta review` (install with `cargo install cargo-insta`) to accept
//! changed snapshots locally. CI sets `INSTA_UPDATE=no` and `CI=true`, so a changed or
//! missing snapshot fails the build instead of being written. Snapshots live in
//! `tests/snapshots/` and are committed.

mod common;

use bitacora_testkit::{markdown_files, relative_to_graph};

/// Snapshot names must be portable file names: keep ASCII alphanumerics only.
fn snapshot_name(rel: &str) -> String {
    rel.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[test]
fn split_lines_snapshots_for_edge_cases() {
    let mut seen = 0;
    for path in markdown_files("edge-cases") {
        let rel = relative_to_graph("edge-cases", &path);
        let bytes = std::fs::read(&path).expect("read fixture");
        let text = String::from_utf8_lossy(&bytes);
        let lines = common::split_lines(&text);
        // `{:#?}` escapes CR, LF and tabs so the snapshot shows the exact line endings.
        insta::assert_snapshot!(snapshot_name(&rel), format!("{rel}\n{lines:#?}"));
        seen += 1;
    }
    assert!(seen > 30, "expected the edge-case graph, saw {seen} files");
}
