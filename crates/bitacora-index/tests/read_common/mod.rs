#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]
//! Helpers for the read-API tests: build an index from in-memory files.

#[path = "../common/mod.rs"]
pub mod common;

use bitacora_config::EffectiveConfig;
use bitacora_index::{BlockRow, Index, IndexReader, Indexer, IndexerOptions};
pub use common::Env;

pub struct Fixture {
    pub env: Env,
    pub index: Index,
    pub reader: IndexReader,
}

/// Write `files` (relative path, text) into a fresh graph and run a cold build.
pub fn indexed(files: &[(&str, &str)]) -> Fixture {
    let env = common::env();
    for (path, text) in files {
        common::write(&env.graph, path, text);
    }
    let index = env.open();
    let ix = Indexer::start(
        &index,
        IndexerOptions::new(&env.graph, EffectiveConfig::default()),
    )
    .expect("indexer");
    let stats = ix.reconcile().expect("reconcile");
    assert!(stats.errors.is_empty(), "{:?}", stats.errors);
    ix.shutdown();
    let reader = index.read_api();
    Fixture { env, index, reader }
}

impl Fixture {
    pub fn page_id(&self, name: &str) -> i64 {
        self.reader
            .page_by_name(name)
            .expect("lookup")
            .unwrap_or_else(|| panic!("page {name}"))
            .id
    }
}

pub fn titles(blocks: &[BlockRow]) -> Vec<String> {
    blocks.iter().map(|b| b.title.clone()).collect()
}
