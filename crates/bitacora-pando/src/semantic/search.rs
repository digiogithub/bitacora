//! Hybrid search (BIT-US-0144): FTS5 results from the local index fused with Pando's semantic
//! results, which are always re-resolved against the local index before they are shown.
//!
//! Pando only ever returns document ids (`bitacora/<graph_id>/<block-uuid>`); the text, the
//! page and the eligibility of a hit come from the index as it is *now*. A hit whose block no
//! longer exists, or is excluded by the current policy (the server copy may be outdated or not
//! yet deleted), is dropped. Pando being off, unreachable, slow or lacking consent never fails
//! the search: the result is lexical only and says why.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use bitacora_index::search::{
    self, DEFAULT_WINDOW, RRF_K, SearchHit as LexicalHit, SearchOptions, Snippet,
};
use bitacora_index::{IndexReader, graph_id};
use pando::Error as PandoError;
use pando::kb::SearchRequest;
use parking_lot::RwLock;

use super::SemanticError;
use super::doc::{ContentPolicy, GraphInfo, SemanticDoc, doc_prefix};
use super::source::{DocSource, IndexSource, SharedPolicy};
use super::worker::{Gate, KbProvider};
use crate::service::PandoService;

/// The largest result count Pando serves for one search.
const REMOTE_MAX: u32 = 20;

/// Tuning of one hybrid search.
#[derive(Debug, Clone)]
pub struct HybridOptions {
    /// Maximum hits returned after fusion.
    pub limit: usize,
    /// How long to wait for Pando before answering with the lexical results only.
    pub timeout: Duration,
    /// Settings of the lexical (FTS5) pass. Its `limit` is raised to at least `limit`.
    pub lexical: SearchOptions,
}

impl Default for HybridOptions {
    fn default() -> Self {
        Self {
            limit: 20,
            timeout: Duration::from_millis(2500),
            lexical: SearchOptions::default(),
        }
    }
}

/// What a hit points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HybridTarget {
    /// A page, matched by title or alias.
    Page {
        /// `pages.id`.
        page_id: i64,
    },
    /// A block, by UUID.
    Block {
        /// Block UUID (lower case).
        uuid: String,
    },
}

/// One fused result.
#[derive(Debug, Clone, PartialEq)]
pub struct HybridHit {
    /// The page or block.
    pub target: HybridTarget,
    /// Page title (for a block, the title of its page).
    pub title: String,
    /// The hit is in a journal.
    pub is_journal: bool,
    /// Text to show; highlights come from the lexical pass when it matched.
    pub snippet: Snippet,
    /// RRF score (higher is better).
    pub score: f64,
    /// 1-based position in the lexical list, when it matched there.
    pub lexical_rank: Option<usize>,
    /// 1-based position in the (re-resolved) semantic list, when it matched there.
    pub semantic_rank: Option<usize>,
    /// The block changed locally since Pando embedded it (the semantic match may be outdated).
    pub stale: bool,
}

/// Why no semantic results are part of an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unavailable {
    /// Pando is off, the graph has no consent or the `semantic_search` feature is disabled.
    Disabled,
    /// Pando is configured but not reachable right now.
    Offline,
    /// Pando did not answer within the timeout.
    Timeout,
    /// Pando answered with an error (redacted text, never content or tokens).
    Failed(String),
}

/// How the semantic half of an answer went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticState {
    /// Semantic results were fused in.
    Used {
        /// Documents Pando returned.
        candidates: usize,
        /// Of those, dropped because the block is gone, excluded or from another graph.
        dropped: usize,
    },
    /// The answer is lexical only.
    Unavailable(Unavailable),
}

/// Answer of [`HybridSearch::search`].
#[derive(Debug, Clone, PartialEq)]
pub struct HybridResults {
    /// Fused hits, best first.
    pub hits: Vec<HybridHit>,
    /// What the semantic half contributed.
    pub semantic: SemanticState,
}

/// How to reach Pando.
#[derive(Clone)]
pub struct Remote {
    /// Runtime the request runs on.
    pub handle: tokio::runtime::Handle,
    /// KB client provider.
    pub kb: KbProvider,
    /// Open while Pando is connected.
    pub gate: Gate,
}

impl std::fmt::Debug for Remote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Remote")
    }
}

/// Hybrid search over one graph. Cheap to clone; calls block, so run them off the UI thread.
#[derive(Clone)]
pub struct HybridSearch {
    reader: IndexReader,
    source: Arc<dyn DocSource>,
    graph_id: String,
    remote: Option<Remote>,
}

impl std::fmt::Debug for HybridSearch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HybridSearch")
            .field("graph_id", &self.graph_id)
            .field("semantic", &self.remote.is_some())
            .finish_non_exhaustive()
    }
}

type RemoteOutcome = Result<Vec<pando::kb::SearchHit>, Unavailable>;

impl HybridSearch {
    /// A searcher resolving hits through `source`; `remote = None` gives lexical-only search.
    #[must_use]
    pub fn new(
        reader: IndexReader,
        source: Arc<dyn DocSource>,
        graph_id: impl Into<String>,
        remote: Option<Remote>,
    ) -> Self {
        Self {
            reader,
            source,
            graph_id: graph_id.into(),
            remote,
        }
    }

    /// The searcher of a graph session. Semantic results are included only when `service` is
    /// active and `policy` is `Some` (the semantic worker runs, so consent and the feature are
    /// on); `policy` is the worker's shared policy so exclusion edits apply immediately.
    ///
    /// # Errors
    /// The graph id could not be derived from `graph_root`.
    pub fn for_session(
        service: Option<&PandoService>,
        policy: Option<SharedPolicy>,
        graph_root: &std::path::Path,
        reader: IndexReader,
    ) -> Result<Self, SemanticError> {
        let id = graph_id(graph_root)?;
        let info = GraphInfo {
            name: graph_root
                .file_name()
                .map_or_else(|| "graph".to_owned(), |n| n.to_string_lossy().into_owned()),
            id: id.clone(),
        };
        let remote = match (service, &policy) {
            (Some(s), Some(_)) => s.handle().map(|handle| {
                let probe = s.probe();
                let kb_probe = probe.clone();
                Remote {
                    handle,
                    kb: Arc::new(move || kb_probe.client().map(|c| c.kb())),
                    gate: Arc::new(move || probe.is_connected()),
                }
            }),
            _ => None,
        };
        let policy = policy.unwrap_or_else(|| Arc::new(RwLock::new(ContentPolicy::default())));
        let source = Arc::new(IndexSource::new(reader.clone(), info, policy));
        Ok(Self::new(reader, source, id, remote))
    }

    /// Searches pages and blocks. The lexical pass always runs; the semantic pass runs when Pando
    /// is reachable and degrades silently (reported in [`HybridResults::semantic`]) otherwise.
    ///
    /// # Errors
    /// The lexical search failed (the index is unreadable).
    pub fn search(
        &self,
        query: &str,
        opts: &HybridOptions,
    ) -> Result<HybridResults, SemanticError> {
        let query = query.trim();
        if query.is_empty() || opts.limit == 0 {
            return Ok(HybridResults {
                hits: Vec::new(),
                semantic: SemanticState::Unavailable(Unavailable::Disabled),
            });
        }
        // Start the remote request first so it overlaps with the local FTS pass.
        let pending = self.start_remote(query, opts);
        let mut lex_opts = opts.lexical.clone();
        lex_opts.limit = lex_opts.limit.max(opts.limit);
        let lexical = self.reader.search(query, &lex_opts)?;
        let (semantic, state) = match pending {
            Err(why) => (Vec::new(), SemanticState::Unavailable(why)),
            Ok(rx) => match rx.recv_timeout(opts.timeout + Duration::from_millis(500)) {
                Ok(Ok(hits)) => {
                    let candidates = hits.len();
                    let (resolved, dropped) = self.resolve(query, hits, &lex_opts);
                    (
                        resolved,
                        SemanticState::Used {
                            candidates,
                            dropped,
                        },
                    )
                }
                Ok(Err(why)) => (Vec::new(), SemanticState::Unavailable(why)),
                Err(_) => (Vec::new(), SemanticState::Unavailable(Unavailable::Timeout)),
            },
        };
        Ok(HybridResults {
            hits: fuse(&lexical, semantic, opts.limit),
            semantic: state,
        })
    }

    /// Sends the Pando request in the background; the receiver yields its outcome.
    fn start_remote(
        &self,
        query: &str,
        opts: &HybridOptions,
    ) -> Result<mpsc::Receiver<RemoteOutcome>, Unavailable> {
        let remote = self.remote.as_ref().ok_or(Unavailable::Disabled)?;
        if !(remote.gate)() {
            return Err(Unavailable::Offline);
        }
        let kb = (remote.kb)().ok_or(Unavailable::Offline)?;
        let mut req = SearchRequest::new(query, REMOTE_MAX);
        req.path_prefix = doc_prefix(&self.graph_id);
        let timeout = opts.timeout;
        let (tx, rx) = mpsc::channel();
        remote.handle.spawn(async move {
            let out: RemoteOutcome = match tokio::time::timeout(timeout, kb.search(&req)).await {
                Err(_) | Ok(Err(PandoError::Timeout)) => Err(Unavailable::Timeout),
                Ok(Err(PandoError::Unreachable(_))) => Err(Unavailable::Offline),
                Ok(Err(e)) => Err(Unavailable::Failed(e.to_string())),
                Ok(Ok(r)) => Ok(r.results),
            };
            let _ = tx.send(out);
        });
        Ok(rx)
    }

    /// Maps Pando hits onto current local blocks; returns the survivors in Pando order (one per
    /// block, best chunk first) and how many candidates were dropped.
    fn resolve(
        &self,
        query: &str,
        hits: Vec<pando::kb::SearchHit>,
        lex: &SearchOptions,
    ) -> (Vec<Resolved>, usize) {
        let prefix = doc_prefix(&self.graph_id);
        let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let term_refs: Vec<&str> = terms.iter().map(String::as_str).collect();
        let mut out: Vec<Resolved> = Vec::new();
        let mut dropped = 0;
        for hit in hits {
            let Some(uuid) = hit
                .file_path
                .strip_prefix(&prefix)
                .filter(|u| !u.is_empty() && !u.contains('/'))
                .map(str::to_ascii_lowercase)
            else {
                dropped += 1;
                continue;
            };
            if out.iter().any(|r| r.uuid == uuid) {
                continue; // another chunk of a block already ranked higher
            }
            // A read failure counts as unknown: a hit we cannot verify is not shown.
            let Some(doc) = self.source.block_doc(&uuid).ok().flatten() else {
                dropped += 1;
                continue;
            };
            let stale = hit
                .metadata
                .get("content_hash")
                .and_then(|h| h.as_str())
                .is_some_and(|h| h != doc.content_hash);
            out.push(Resolved {
                uuid,
                title: doc
                    .metadata
                    .get("page")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned(),
                is_journal: doc.metadata.contains_key("journal_day"),
                snippet: search::build_snippet(
                    body_of(&doc),
                    &term_refs,
                    lex.remove_accents,
                    DEFAULT_WINDOW,
                ),
                stale,
            });
        }
        (out, dropped)
    }
}

struct Resolved {
    uuid: String,
    title: String,
    is_journal: bool,
    snippet: Snippet,
    stale: bool,
}

/// The block text of a document without its `# page` / `> breadcrumb` header.
fn body_of(doc: &SemanticDoc) -> &str {
    doc.text
        .split_once("\n\n")
        .map_or(doc.text.as_str(), |(_, body)| body)
        .trim_end()
}

#[allow(clippy::cast_precision_loss)]
fn rrf(rank: usize) -> f64 {
    1.0 / (RRF_K + rank as f64)
}

/// Reciprocal Rank Fusion of the lexical list and the re-resolved semantic list.
fn fuse(lexical: &[LexicalHit], semantic: Vec<Resolved>, limit: usize) -> Vec<HybridHit> {
    let mut by_key: HashMap<String, usize> = HashMap::new();
    let mut hits: Vec<HybridHit> = Vec::new();
    for (i, h) in lexical.iter().enumerate() {
        let rank = i + 1;
        let (key, hit) = match h {
            LexicalHit::Page {
                page_id,
                title,
                is_journal,
                snippet,
                ..
            } => (
                format!("p:{page_id}"),
                HybridHit {
                    target: HybridTarget::Page { page_id: *page_id },
                    title: title.clone(),
                    is_journal: *is_journal,
                    snippet: snippet.clone(),
                    score: rrf(rank),
                    lexical_rank: Some(rank),
                    semantic_rank: None,
                    stale: false,
                },
            ),
            LexicalHit::Block {
                uuid,
                page_title,
                is_journal,
                snippet,
                ..
            } => {
                let uuid = uuid.to_ascii_lowercase();
                (
                    format!("b:{uuid}"),
                    HybridHit {
                        target: HybridTarget::Block { uuid },
                        title: page_title.clone(),
                        is_journal: *is_journal,
                        snippet: snippet.clone(),
                        score: rrf(rank),
                        lexical_rank: Some(rank),
                        semantic_rank: None,
                        stale: false,
                    },
                )
            }
        };
        if by_key.contains_key(&key) {
            continue;
        }
        by_key.insert(key, hits.len());
        hits.push(hit);
    }
    for (i, r) in semantic.into_iter().enumerate() {
        let rank = i + 1;
        let key = format!("b:{}", r.uuid);
        if let Some(&ix) = by_key.get(&key) {
            hits[ix].score += rrf(rank);
            hits[ix].semantic_rank = Some(rank);
            hits[ix].stale = r.stale;
        } else {
            by_key.insert(key, hits.len());
            hits.push(HybridHit {
                target: HybridTarget::Block { uuid: r.uuid },
                title: r.title,
                is_journal: r.is_journal,
                snippet: r.snippet,
                score: rrf(rank),
                lexical_rank: None,
                semantic_rank: Some(rank),
                stale: r.stale,
            });
        }
    }
    // Stable sort: ties keep the lexical-first insertion order.
    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(uuid: &str) -> LexicalHit {
        LexicalHit::Block {
            block_id: 1,
            uuid: uuid.to_owned(),
            page_id: 1,
            page_title: "P".into(),
            is_journal: false,
            snippet: Snippet {
                text: uuid.into(),
                highlights: vec![],
            },
            score: 1.0,
        }
    }

    fn sem(uuid: &str) -> Resolved {
        Resolved {
            uuid: uuid.to_owned(),
            title: "P".into(),
            is_journal: false,
            snippet: Snippet {
                text: uuid.into(),
                highlights: vec![],
            },
            stale: false,
        }
    }

    fn ids(h: &[HybridHit]) -> Vec<String> {
        h.iter()
            .map(|h| match &h.target {
                HybridTarget::Block { uuid } => uuid.clone(),
                HybridTarget::Page { page_id } => format!("page{page_id}"),
            })
            .collect()
    }

    #[test]
    fn blocks_in_both_lists_rise_above_single_list_hits() {
        let hits = fuse(
            &[lex("a"), lex("b"), lex("c")],
            vec![sem("c"), sem("d")],
            10,
        );
        // c: 1/63 + 1/61 beats a: 1/61 and d: 1/62.
        assert_eq!(ids(&hits), ["c", "a", "b", "d"]);
        let c = &hits[0];
        assert_eq!((c.lexical_rank, c.semantic_rank), (Some(3), Some(1)));
        assert_eq!(hits[3].lexical_rank, None);
    }

    #[test]
    fn lexical_only_keeps_order_and_limit_truncates() {
        let hits = fuse(&[lex("a"), lex("b"), lex("c")], vec![], 2);
        assert_eq!(ids(&hits), ["a", "b"]);
    }

    #[test]
    fn semantic_only_results_are_kept() {
        let hits = fuse(&[], vec![sem("x"), sem("y")], 10);
        assert_eq!(ids(&hits), ["x", "y"]);
    }

    #[test]
    fn header_is_stripped_from_the_body() {
        let doc = SemanticDoc {
            doc_id: "d".into(),
            block_uuid: "u".into(),
            file_path: "p".into(),
            text: "# Page\n> a > b\n\nthe body\nsecond line\n".into(),
            metadata: serde_json::Map::new(),
            tags: vec![],
            content_hash: String::new(),
        };
        assert_eq!(body_of(&doc), "the body\nsecond line");
    }
}
