//! `Indexer::set_config` (config hot reload) and `Indexer::reindex`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use bitacora_config::EffectiveConfig;
use bitacora_index::{Indexer, IndexerOptions, RebuildKind};
use common::{env_for, write};

fn names(index: &bitacora_index::Index) -> Vec<String> {
    let conn = index.reader().expect("reader");
    let mut stmt = conn
        .prepare("SELECT path FROM files ORDER BY path")
        .expect("prepare");
    stmt.query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows")
}

#[test]
fn set_config_flags_a_reparse_only_when_the_hash_changes() {
    let t = tempfile::tempdir().expect("tmp");
    let g = t.path().join("g");
    write(&g, "pages/a.md", "- a\n");
    write(&g, "secret/s.md", "- s\n");
    let env = env_for(g.clone());
    let index = env.open();
    let ix =
        Indexer::start(&index, IndexerOptions::new(&g, EffectiveConfig::default())).expect("start");
    ix.reconcile().expect("cold");
    assert_eq!(ix.pending_rebuild(), RebuildKind::None);
    assert_eq!(names(&index), ["pages/a.md", "secret/s.md"]);

    // Same config, and a change the parser does not depend on: nothing pending.
    let same = EffectiveConfig::default();
    assert_eq!(ix.set_config(same).expect("same"), RebuildKind::None);
    let editor = EffectiveConfig::from_texts(None, Some("{:preferred-workflow :now}"));
    assert_eq!(ix.set_config(editor).expect("editor"), RebuildKind::None);

    // `:hidden` changes what is indexed.
    let hidden = EffectiveConfig::from_texts(None, Some("{:hidden [\"/secret\"]}"));
    assert_eq!(
        ix.set_config(hidden).expect("hidden"),
        RebuildKind::FullReparse
    );
    ix.reconcile().expect("reparse");
    assert_eq!(ix.pending_rebuild(), RebuildKind::None);
    assert_eq!(names(&index), ["pages/a.md"]);
    drop(ix);
    drop(index);

    // The new hash was recorded: reopening with the new config needs no rebuild.
    let index = env.open_with(&EffectiveConfig::from_texts(
        None,
        Some("{:hidden [\"/secret\"]}"),
    ));
    assert_eq!(index.outcome().rebuild, RebuildKind::None);
}

#[test]
fn reindex_rebuilds_from_the_files() {
    let t = tempfile::tempdir().expect("tmp");
    let g = t.path().join("g");
    write(&g, "pages/a.md", "- a\n");
    let env = env_for(g.clone());
    let index = env.open();
    let ix =
        Indexer::start(&index, IndexerOptions::new(&g, EffectiveConfig::default())).expect("start");
    ix.reconcile().expect("cold");
    write(&g, "pages/b.md", "- b\n");
    let stats = ix.reindex().expect("reindex");
    assert!(stats.cold_build);
    assert_eq!(names(&index), ["pages/a.md", "pages/b.md"]);
}
