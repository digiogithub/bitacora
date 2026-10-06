//! Test helpers: a real indexed graph in a temp folder.

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use bitacora_config::EffectiveConfig;
use bitacora_core::graph_path::GraphPath;
use bitacora_index::{
    FsChange, Index, IndexEvent, IndexLocation, Indexer, IndexerOptions, OpenOptions, config_hash,
};

use crate::data::{GraphHandle, ViewSettings};

/// A graph folder plus its live index.
pub struct TestGraph {
    graph: tempfile::TempDir,
    _data: tempfile::TempDir,
    _index: Index,
    indexer: Option<Indexer>,
    events: Receiver<IndexEvent>,
    /// Read handle over the index.
    pub handle: GraphHandle,
}

impl std::fmt::Debug for TestGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestGraph")
            .field("root", &self.handle.root)
            .finish_non_exhaustive()
    }
}

impl TestGraph {
    /// Writes `files` (graph-relative path, content), indexes them and keeps indexing alive.
    pub fn new(files: &[(&str, &str)]) -> Self {
        Self::with_config(files, "{}")
    }

    /// Like [`TestGraph::new`] with the given `config.edn` text.
    pub fn with_config(files: &[(&str, &str)], config: &str) -> Self {
        let graph = tempfile::tempdir().expect("graph dir");
        let data = tempfile::tempdir().expect("data dir");
        let root = graph.path().canonicalize().expect("canonical root");
        std::fs::create_dir_all(root.join("logseq")).expect("logseq dir");
        std::fs::write(root.join("logseq/config.edn"), config).expect("config");
        for (path, content) in files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().expect("parent")).expect("dirs");
            std::fs::write(file, content).expect("write file");
        }
        let cfg = EffectiveConfig::from_texts(None, Some(config));
        let location = IndexLocation::in_data_dir(data.path(), &root).expect("location");
        let index =
            Index::open(location, OpenOptions::new(&root, config_hash(&cfg))).expect("index");
        let settings = Arc::new(ViewSettings::from_config(&cfg));
        let indexer = Indexer::start(&index, IndexerOptions::new(&root, cfg)).expect("indexer");
        let events = indexer.subscribe();
        indexer.reconcile().expect("reconcile");
        while events.try_recv().is_ok() {}
        let handle = GraphHandle {
            reader: index.read_api(),
            root,
            settings,
        };
        Self {
            graph,
            _data: data,
            _index: index,
            indexer: Some(indexer),
            events,
            handle,
        }
    }

    /// The graph folder.
    pub fn path(&self) -> &Path {
        self.graph.path()
    }

    /// Writes a file and indexes it; returns the resulting index event.
    pub fn rewrite(&self, path: &str, content: &str) -> IndexEvent {
        let file = self.handle.root.join(path);
        std::fs::create_dir_all(file.parent().expect("parent")).expect("dirs");
        std::fs::write(file, content).expect("write file");
        let rel = GraphPath::new(path).expect("graph path");
        let indexer = self.indexer.as_ref().expect("indexer");
        indexer.handle(&FsChange::Modified(rel)).expect("handle");
        loop {
            let event = self
                .events
                .recv_timeout(Duration::from_secs(10))
                .expect("index event");
            if matches!(event, IndexEvent::FileReplaced { .. }) {
                return event;
            }
        }
    }
}

impl Drop for TestGraph {
    fn drop(&mut self) {
        if let Some(indexer) = self.indexer.take() {
            indexer.shutdown();
        }
    }
}
