//! `logseq/config.edn` hot reload, the journal-template rule (BIT-SP-0002.R17) and `reindex()`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use bitacora_config::EffectiveConfig;
use bitacora_runtime::{RuntimeEvent, Session};
use common::{config, graph_with, index_has, wait_for};

const WAIT: Duration = Duration::from_secs(20);

/// Writes `text` atomically so the watcher never sees a half-written file.
fn replace(path: &Path, text: &str) {
    let tmp = path.with_extension("tmp-test");
    std::fs::write(&tmp, text).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

fn wait_reloaded(rx: &Receiver<RuntimeEvent>) -> (Arc<EffectiveConfig>, bool) {
    wait_for("ConfigReloaded", WAIT, || {
        rx.try_iter().find_map(|e| match e {
            RuntimeEvent::ConfigReloaded { config, reindexed } => Some((config, reindexed)),
            _ => None,
        })
    })
}

fn open(tmp: &Path, files: &[(&str, &str)]) -> (std::path::PathBuf, Session) {
    let graph = graph_with(tmp, files);
    let mut cfg = config(&graph, &tmp.join("data"));
    cfg.debounce = None;
    (graph, Session::open(cfg).unwrap())
}

#[test]
fn parser_relevant_change_reloads_and_reindexes() {
    let tmp = tempfile::tempdir().unwrap();
    let (graph, s) = open(
        tmp.path(),
        &[
            ("logseq/config.edn", "{:preferred-workflow :now}\n"),
            ("pages/p.md", "- Alpha\n"),
            ("secret/s.md", "- Hidden words\n"),
        ],
    );
    assert!(index_has(&s, "Hidden words"));
    assert_eq!(s.current_config().hidden().len(), 0);
    let events = s.subscribe();

    replace(
        &graph.join("logseq/config.edn"),
        "{:preferred-workflow :todo\n :hidden [\"/secret\"]}\n",
    );
    let (cfg, reindexed) = wait_reloaded(&events);
    assert!(reindexed, "`:hidden` is part of the config hash");
    assert_eq!(cfg.hidden(), vec!["/secret".to_owned()]);
    assert_eq!(s.current_config().hidden(), cfg.hidden());
    assert!(
        !index_has(&s, "Hidden words"),
        "hidden files left the index"
    );
    assert!(index_has(&s, "Alpha"));
    // The hash is recorded: a fresh open with the new config needs no rebuild.
    assert!(s.shutdown(Duration::from_secs(10)).is_clean());
    let again = Session::open(config(&graph, &tmp.path().join("data"))).unwrap();
    assert_eq!(
        again.open_stats().map(|st| st.parsed),
        Some(0),
        "second open reparses nothing"
    );
}

#[test]
fn editor_only_change_does_not_reindex_and_invalid_config_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let (graph, s) = open(
        tmp.path(),
        &[
            ("logseq/config.edn", "{:preferred-workflow :now}\n"),
            ("pages/p.md", "- Alpha\n"),
        ],
    );
    let events = s.subscribe();
    replace(
        &graph.join("logseq/config.edn"),
        "{:preferred-workflow :todo :editor/logical-outdenting? true}\n",
    );
    let (cfg, reindexed) = wait_reloaded(&events);
    assert!(!reindexed);
    assert_eq!(
        cfg.get("editor/logical-outdenting?")
            .and_then(bitacora_config::Edn::as_bool),
        Some(true)
    );

    replace(&graph.join("logseq/config.edn"), "{:preferred-workflow ");
    let msg = wait_for("ConfigReloadFailed", WAIT, || {
        events.try_iter().find_map(|e| match e {
            RuntimeEvent::ConfigReloadFailed { message } => Some(message),
            _ => None,
        })
    });
    assert!(!msg.is_empty());
    assert_eq!(
        s.current_config()
            .get("editor/logical-outdenting?")
            .and_then(bitacora_config::Edn::as_bool),
        Some(true),
        "previous config stays active"
    );
}

#[test]
fn journal_equal_to_the_default_template_is_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    let (graph, s) = open(
        tmp.path(),
        &[
            (
                "logseq/config.edn",
                "{:default-templates {:journals \"daily\"}}\n",
            ),
            (
                "pages/Templates.md",
                "- Daily\n  template:: daily\n\t- Morning plan\n\t- Evening review\n",
            ),
            ("pages/p.md", "- Alpha\n"),
        ],
    );
    let events = s.subscribe();
    let journals = graph.join("journals");
    std::fs::create_dir_all(&journals).unwrap();
    // Template-only journal (trailing whitespace differs): ignored.
    replace(
        &journals.join("2026_10_01.md"),
        "- Morning plan\n- Evening review\n\n",
    );
    // A journal with real content right after: reported, proving the first was seen and skipped.
    replace(
        &journals.join("2026_10_02.md"),
        "- Morning plan\n- Real entry\n",
    );
    wait_for("real journal indexed", WAIT, || {
        index_has(&s, "Real entry").then_some(())
    });
    let conn = s.index_reader().unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE path = 'journals/2026_10_01.md'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0, "template-only journal was not indexed");
    let paths: Vec<String> = events
        .try_iter()
        .filter_map(|e| match e {
            RuntimeEvent::ExternalChange { path, .. } => Some(path),
            _ => None,
        })
        .collect();
    assert!(!paths.iter().any(|p| p.ends_with("2026_10_01.md")));
}

#[test]
fn reindex_rebuilds_without_reopening() {
    let tmp = tempfile::tempdir().unwrap();
    let (graph, s) = open(tmp.path(), &[("pages/p.md", "- Alpha\n")]);
    replace(&graph.join("pages/q.md"), "- Offline words\n");
    let stats = s.reindex().unwrap();
    assert!(stats.cold_build);
    assert!(index_has(&s, "Offline words"));
    assert!(index_has(&s, "Alpha"));
    // Still live afterwards: a new external edit is indexed.
    replace(&graph.join("pages/r.md"), "- After reindex\n");
    wait_for("incremental after reindex", WAIT, || {
        index_has(&s, "After reindex").then_some(())
    });
    assert!(s.shutdown(Duration::from_secs(10)).is_clean());
}
