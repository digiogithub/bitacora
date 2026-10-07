//! Adapter exposing the session's hybrid search to the MCP semantic tools (BIT-SP-0010.R5).

use std::time::Duration;

use bitacora_mcp::{SemanticFailure, SemanticMatch, SemanticProvider};
use bitacora_pando::semantic::{
    HybridOptions, HybridSearch, HybridTarget, SemanticState, Unavailable,
};

/// Semantic candidates for MCP: the semantic half of [`HybridSearch`], blocks only, best first.
pub(crate) struct HybridProvider(pub HybridSearch);

impl SemanticProvider for HybridProvider {
    fn search(&self, query: &str, limit: usize) -> Result<Vec<SemanticMatch>, SemanticFailure> {
        let opts = HybridOptions {
            limit: limit.max(1),
            timeout: Duration::from_millis(2500),
            ..HybridOptions::default()
        };
        let res = self
            .0
            .search(query, &opts)
            .map_err(|e| SemanticFailure::Unavailable(e.to_string()))?;
        match res.semantic {
            SemanticState::Unavailable(Unavailable::Disabled) => Err(SemanticFailure::Disabled),
            SemanticState::Unavailable(Unavailable::Offline) => {
                Err(SemanticFailure::Unavailable("Pando is offline".into()))
            }
            SemanticState::Unavailable(Unavailable::Timeout) => {
                Err(SemanticFailure::Unavailable("Pando timed out".into()))
            }
            SemanticState::Unavailable(Unavailable::Failed(why)) => {
                Err(SemanticFailure::Unavailable(why))
            }
            SemanticState::Used { .. } => {
                let mut hits: Vec<_> = res
                    .hits
                    .into_iter()
                    .filter_map(|h| match (&h.target, h.semantic_rank) {
                        (HybridTarget::Block { uuid }, Some(rank)) => Some((
                            rank,
                            SemanticMatch {
                                uuid: uuid.clone(),
                                score: h.score,
                                stale: h.stale,
                            },
                        )),
                        _ => None,
                    })
                    .collect();
                hits.sort_by_key(|(rank, _)| *rank);
                Ok(hits.into_iter().map(|(_, m)| m).collect())
            }
        }
    }
}
