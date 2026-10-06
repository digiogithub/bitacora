//! Page rename through a real session: the cascade finds referring files through the index
//! (BIT-US-0061, BIT-US-0082).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use bitacora_core::editor::{MergeMode, RenameRequest};
use bitacora_core::queue::Source;
use bitacora_runtime::Session;
use common::{config, graph_with};

#[test]
fn rename_cascades_through_the_index() {
    let tmp = tempfile::tempdir().unwrap();
    let graph = graph_with(
        tmp.path(),
        &[
            ("pages/Old.md", "- old body\n"),
            ("pages/Old___child.md", "- child\n"),
            (
                "pages/A.md",
                "- keep   as is\n- see [[Old]] and [[Old/child]]\n",
            ),
            ("pages/B.md", "- nothing\n"),
            (
                "logseq/config.edn",
                "{:file/name-format :triple-lowbar\n :favorites [\"Old\"]}\n",
            ),
        ],
    );
    let s = Session::open(config(&graph, &tmp.path().join("data"))).unwrap();
    let req = RenameRequest {
        from: "Old".into(),
        to: "Fresh".into(),
        config: std::sync::Arc::new(s.config().clone()),
        lookup: Some(s.ref_lookup()),
        merge: MergeMode::Refuse,
    };
    let report = s.queue().rename_page(Source::Ui, req).unwrap();
    assert_eq!(report.renamed.len(), 2, "{report:?}");
    assert_eq!(report.rewritten_blocks, 1);
    s.queue().flush(Source::Ui).unwrap();

    assert_eq!(
        std::fs::read_to_string(graph.join("pages/A.md")).unwrap(),
        "- keep   as is\n- see [[Fresh]] and [[Fresh/child]]\n"
    );
    assert!(graph.join("pages/Fresh.md").exists());
    assert!(graph.join("pages/Fresh___child.md").exists());
    assert!(!graph.join("pages/Old.md").exists());
    assert_eq!(
        std::fs::read_to_string(graph.join("pages/B.md")).unwrap(),
        "- nothing\n"
    );
    assert_eq!(
        std::fs::read_to_string(graph.join("logseq/config.edn")).unwrap(),
        "{:file/name-format :triple-lowbar\n :favorites [\"Fresh\"]}\n"
    );
    assert!(s.shutdown(Duration::from_secs(10)).is_clean());
}
