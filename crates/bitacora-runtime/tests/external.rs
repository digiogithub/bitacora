//! External edits of loaded pages through a real session and temp directory: reload with stable
//! block ids, block-level merge into dirty pages and the conflict notice (BIT-US-0068,
//! BIT-US-0069, BIT-US-0070).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use bitacora_core::editor::{Cmd, DiffKind};
use bitacora_core::queue::{Keep, QueueEvent, Request, Response, Source};
use bitacora_runtime::{RuntimeEvent, Session};
use common::{blocks, config, graph_with, wait_for};

const PAGE: &str = "- Alpha block with some words in it\n- Beta block stays as it is\n- Gamma block is the last one here\n";

fn open(tmp: &std::path::Path) -> (std::path::PathBuf, Session) {
    let graph = graph_with(tmp, &[("pages/p.md", PAGE)]);
    let mut cfg = config(&graph, &tmp.join("data"));
    cfg.debounce = None; // only explicit flushes write
    (graph.clone(), Session::open(cfg).unwrap())
}

fn read(graph: &std::path::Path) -> String {
    std::fs::read_to_string(graph.join("pages/p.md")).unwrap()
}

/// Replaces the page file atomically: a plain `fs::write` truncates first, and under load the
/// watcher can report the empty intermediate file as a complete external change.
fn replace_page(graph: &std::path::Path, text: &str) {
    let tmp = graph.join("pages/p.md.tmp-test");
    std::fs::write(&tmp, text).unwrap();
    std::fs::rename(&tmp, graph.join("pages/p.md")).unwrap();
}

fn set(s: &Session, id: bitacora_core::editor::BlockId, text: &str) {
    s.queue()
        .run(
            Source::Ui,
            "edit",
            Cmd::SetText {
                id,
                text: text.into(),
            },
        )
        .unwrap();
}

#[test]
fn clean_page_reloads_with_stable_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let (graph, s) = open(tmp.path());
    let key = s.open_page("pages/p.md").unwrap();
    let before = blocks(&s, &key);

    replace_page(
        &graph,
        "- Alpha block with some words in it\n- Beta block stays as it is, edited\n- Gamma block is the last one here\n- A brand new block appended later\n",
    );
    // Wait for the final state, not for the first event: a stale event for the file as created
    // can still arrive after `open_page` and must not end the wait early.
    wait_for("reload", Duration::from_secs(20), || {
        (blocks(&s, &key).len() == 4).then_some(())
    });
    let after = blocks(&s, &key);
    assert_eq!(after.len(), 4);
    for i in 0..3 {
        assert_eq!(after[i].0, before[i].0, "block {i} keeps its id");
    }
    assert_eq!(after[1].1, "Beta block stays as it is, edited");
    assert!(s.shutdown(Duration::from_secs(10)).is_clean());
}

#[test]
fn external_edit_of_another_block_merges_into_a_dirty_page() {
    let tmp = tempfile::tempdir().unwrap();
    let (graph, s) = open(tmp.path());
    let events = s.subscribe();
    let key = s.open_page("pages/p.md").unwrap();
    let before = blocks(&s, &key);
    set(&s, before[0].0, "Alpha block with MY words in it");

    replace_page(
        &graph,
        "- Alpha block with some words in it\n- Beta block stays as it is\n- Gamma block is the last one here, THEIRS\n",
    );
    wait_for("merge", Duration::from_secs(10), || {
        events.try_iter().find_map(|e| match e {
            RuntimeEvent::ExternalMerged { .. } => Some(()),
            _ => None,
        })
    });
    let after = blocks(&s, &key);
    assert_eq!(after[0].0, before[0].0);
    assert_eq!(after[0].1, "Alpha block with MY words in it");
    assert_eq!(after[2].1, "Gamma block is the last one here, THEIRS");

    let r = s.queue().flush(Source::Ui).unwrap();
    assert!(r.is_complete(), "{r:?}");
    assert_eq!(
        read(&graph),
        "- Alpha block with MY words in it\n- Beta block stays as it is\n- Gamma block is the last one here, THEIRS\n"
    );
    assert!(s.shutdown(Duration::from_secs(10)).is_clean());
}

fn conflicted(
    tmp: &std::path::Path,
) -> (std::path::PathBuf, Session, bitacora_core::graph::PageKey) {
    let (graph, s) = open(tmp);
    let events = s.subscribe();
    let key = s.open_page("pages/p.md").unwrap();
    let id = blocks(&s, &key)[0].0;
    set(&s, id, "Alpha block with MY words in it");
    replace_page(
        &graph,
        "- Alpha block with THEIR words in it\n- Beta block stays as it is\n- Gamma block is the last one here\n",
    );
    wait_for("conflict", Duration::from_secs(10), || {
        events.try_iter().find_map(|e| match e {
            RuntimeEvent::Queue(QueueEvent::PageConflicted(n)) => Some(n),
            _ => None,
        })
    });
    (graph, s, key)
}

#[test]
fn conflicting_edit_keeps_both_versions_and_writes_no_markers() {
    let tmp = tempfile::tempdir().unwrap();
    let (graph, s, key) = conflicted(tmp.path());
    let notice = s.queue().conflict(&key).expect("notice");
    assert!(notice.base_available);
    assert!(notice.diff.iter().any(|d| d.kind == DiffKind::Changed
        && d.mine.as_deref().is_some_and(|m| m.contains("MY words"))
        && d.disk.as_deref().is_some_and(|m| m.contains("THEIR words"))));
    // Ours stays in memory, the file keeps theirs, nothing is written.
    assert_eq!(blocks(&s, &key)[0].1, "Alpha block with MY words in it");
    let r = s.queue().flush(Source::Ui).unwrap();
    assert_eq!(r.conflicts, vec![key.clone()]);
    let on_disk = read(&graph);
    assert!(on_disk.contains("THEIR words") && !on_disk.contains("<<<<<<<"));

    // Keep mine overwrites after backing the disk version up.
    let r = match s
        .queue()
        .execute(
            Source::Ui,
            Request::Resolve {
                key: key.clone(),
                keep: Keep::Mine,
            },
        )
        .unwrap()
    {
        Response::Resolved(r, _) => r,
        other => panic!("{other:?}"),
    };
    assert!(r.is_complete(), "{r:?}");
    assert!(!r.backups.is_empty());
    assert!(read(&graph).contains("MY words"));
    assert!(s.queue().conflict(&key).is_none());
    assert!(s.shutdown(Duration::from_secs(10)).is_clean());
}

#[test]
fn take_disk_resolves_the_conflict() {
    let tmp = tempfile::tempdir().unwrap();
    let (graph, s, key) = conflicted(tmp.path());
    let ids: Vec<_> = blocks(&s, &key).iter().map(|b| b.0).collect();
    s.queue()
        .execute(
            Source::Ui,
            Request::Resolve {
                key: key.clone(),
                keep: Keep::Disk,
            },
        )
        .unwrap();
    let after = blocks(&s, &key);
    assert_eq!(after[0].1, "Alpha block with THEIR words in it");
    assert_eq!(after.iter().map(|b| b.0).collect::<Vec<_>>(), ids);
    assert!(read(&graph).contains("THEIR words"));
    assert!(s.queue().conflict(&key).is_none());
    assert!(s.shutdown(Duration::from_secs(10)).is_clean());
}
