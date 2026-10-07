//! Wiring of the semantic worker into a graph session (used by `bitacora-runtime`).

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use bitacora_config::{PandoFeature, PandoMode};
use bitacora_index::{IndexEvent, IndexReader, graph_id};
use parking_lot::RwLock;

use super::SemanticError;
use super::doc::{ContentPolicy, GraphInfo};
use super::ledger::Ledger;
use super::source::{IndexSource, SharedPolicy};
use super::worker::{SemanticWorker, SenderConfig, attach};
use crate::service::{PandoOptions, PandoService};

/// File name of the sync ledger inside the per-graph data directory (next to `index.sqlite`).
pub const LEDGER_FILE: &str = "semantic.sqlite";

/// What a session hands over to start semantic sync.
pub struct SessionInputs<'a> {
    /// Canonical graph root.
    pub graph_root: &'a Path,
    /// The per-graph data directory (`IndexLocation::dir`).
    pub data_dir: &'a Path,
    /// Read access to the index.
    pub reader: IndexReader,
    /// A fresh subscription to the index events.
    pub events: Receiver<IndexEvent>,
}

impl std::fmt::Debug for SessionInputs<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionInputs")
            .field("graph_root", &self.graph_root)
            .finish_non_exhaustive()
    }
}

/// The identity of the remote the ledger describes: documents in one Pando server are not in
/// another, so a different remote starts from an empty ledger.
fn remote_key(opts: &PandoOptions, graph_id: &str) -> String {
    match opts.settings.mode {
        // A managed server belongs to this graph, whatever port it listens on this time.
        PandoMode::Managed => format!("managed:{graph_id}"),
        _ => opts.settings.rest_url.trim_end_matches('/').to_owned(),
    }
}

/// Starts semantic sync for the session, or returns `Ok(None)` when it must not run: the
/// integration is inactive or without consent for this graph (the service has no runtime then),
/// or the `semantic_search` feature is off.
///
/// # Errors
/// The ledger could not be opened or the worker could not start. Callers should log and carry on:
/// semantic search is optional and must never stop a graph from opening.
pub fn start_session(
    service: &PandoService,
    opts: &PandoOptions,
    inputs: SessionInputs<'_>,
) -> Result<Option<SemanticWorker>, SemanticError> {
    if !opts.settings.feature_enabled(PandoFeature::SemanticSearch) {
        return Ok(None);
    }
    if service.handle().is_none() {
        return Ok(None);
    }
    let id = graph_id(inputs.graph_root)?;
    let graph = GraphInfo {
        name: inputs
            .graph_root
            .file_name()
            .map_or_else(|| "graph".to_owned(), |n| n.to_string_lossy().into_owned()),
        id: id.clone(),
    };
    let policy: SharedPolicy = Arc::new(RwLock::new(ContentPolicy::from_consent(
        &opts.settings.consent(&opts.graph_key()),
    )));
    let ledger = Arc::new(Ledger::open(
        &inputs.data_dir.join(LEDGER_FILE),
        &remote_key(opts, &id),
    )?);
    let source = Arc::new(IndexSource::new(inputs.reader, graph, Arc::clone(&policy)));
    attach(
        service,
        source,
        policy,
        ledger,
        inputs.events,
        SenderConfig::default(),
    )
}
