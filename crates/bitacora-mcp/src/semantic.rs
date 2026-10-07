//! Semantic search tools (`semantic_search`, `related_blocks`; BIT-SP-0010.R5).
//!
//! The MCP crate does not know Pando: a [`SemanticProvider`] (implemented by `bitacora-runtime`
//! over the hybrid search) returns candidate block uuids, and the tools resolve every one of them
//! through the *calling token's* reader. A block that reader cannot see (read exclusions, ADR-031)
//! is dropped, so semantic results never leak what the token could not read directly. Text and
//! page titles always come from the local graph, never from the provider.

use std::fmt::Write as _;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::reader::GraphReader;
use crate::render::{Code, ToolError, ToolResult, limit, output, strip_properties};
use crate::tools::check_graph;

/// One candidate block from the semantic index, best first.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticMatch {
    /// Block UUID.
    pub uuid: String,
    /// Fused relevance (higher is better).
    pub score: f64,
    /// The block changed locally since it was embedded.
    pub stale: bool,
}

/// Why a provider could not answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticFailure {
    /// Semantic search is not enabled for this graph (Pando off, no consent, feature off).
    Disabled,
    /// Enabled but not answering right now (offline, timeout, error); the text is redacted.
    Unavailable(String),
}

/// Source of semantic candidates. Synchronous: called from `spawn_blocking`.
pub trait SemanticProvider: Send + Sync {
    /// Up to `limit` blocks semantically close to `query`.
    ///
    /// # Errors
    /// [`SemanticFailure`] when semantic search is disabled or unavailable.
    fn search(&self, query: &str, limit: usize) -> Result<Vec<SemanticMatch>, SemanticFailure>;
}

/// Arguments of `semantic_search`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct SemanticSearchArgs {
    /// Natural-language query.
    pub query: String,
    /// Maximum hits (default 10, max 50).
    pub limit: Option<u32>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Arguments of `related_blocks`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct RelatedBlocksArgs {
    /// Block UUID to find neighbours of.
    pub block_uuid: String,
    /// Maximum hits (default 10, max 50).
    pub limit: Option<u32>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// One semantic hit.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct SemanticHit {
    pub uuid: String,
    pub page: String,
    /// Current block text (first lines, local copy).
    pub snippet: String,
    pub score: f64,
    /// The semantic match may be outdated (the block was edited after it was embedded).
    pub stale: bool,
}

/// Result of both tools.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct SemanticOut {
    pub query: String,
    pub hits: Vec<SemanticHit>,
}

const SNIPPET_CHARS: usize = 300;
const QUERY_CHARS: usize = 500;

fn failure(f: SemanticFailure) -> ToolError {
    match f {
        SemanticFailure::Disabled => ToolError::new(
            Code::SemanticDisabled,
            "semantic search is not enabled for this graph",
        ),
        SemanticFailure::Unavailable(why) => ToolError::new(
            Code::SemanticUnavailable,
            format!("semantic search is temporarily unavailable: {why}"),
        ),
    }
}

fn cut(s: &str, max: usize) -> String {
    let flat = s.replace('\n', " ");
    match flat.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", flat[..i].trim_end()),
        None => flat,
    }
}

/// Resolve candidates through the token's reader, dropping anything it cannot read.
fn resolve(
    r: &dyn GraphReader,
    matches: Vec<SemanticMatch>,
    skip: Option<&str>,
    n: usize,
) -> Result<Vec<SemanticHit>, ToolError> {
    let mut hits = Vec::new();
    for m in matches {
        if skip.is_some_and(|s| s.eq_ignore_ascii_case(&m.uuid)) {
            continue;
        }
        let Some(mut b) = r.block(&m.uuid)? else {
            continue;
        };
        strip_properties(&mut b);
        hits.push(SemanticHit {
            uuid: b.uuid,
            page: b.page,
            snippet: cut(&b.content, SNIPPET_CHARS),
            score: m.score,
            stale: m.stale,
        });
        if hits.len() >= n {
            break;
        }
    }
    Ok(hits)
}

fn render(title: &str, query: String, hits: Vec<SemanticHit>) -> ToolResult {
    let mut md = format!("{title}:\n\n");
    for h in &hits {
        let stale = if h.stale { ", possibly outdated" } else { "" };
        let _ = writeln!(
            md,
            "- {}  _(page: [[{}]], id: {}{stale})_",
            h.snippet, h.page, h.uuid
        );
    }
    if hits.is_empty() {
        md.push_str("No results.\n");
    }
    output(&SemanticOut { query, hits }, md)
}

pub(crate) fn semantic_search(
    r: &dyn GraphReader,
    provider: Option<Arc<dyn SemanticProvider>>,
    a: SemanticSearchArgs,
) -> ToolResult {
    check_graph(r, a.graph.as_deref())?;
    if a.query.trim().is_empty() {
        return Err(ToolError::invalid("`query` must not be empty"));
    }
    let provider = provider.ok_or_else(|| failure(SemanticFailure::Disabled))?;
    let n = limit(a.limit, 10, 50);
    // Over-fetch: hidden blocks are dropped after the provider answered.
    let found = provider.search(a.query.trim(), n * 2).map_err(failure)?;
    let hits = resolve(r, found, None, n)?;
    render(
        &format!("Semantic results for `{}`", a.query),
        a.query,
        hits,
    )
}

pub(crate) fn related_blocks(
    r: &dyn GraphReader,
    provider: Option<Arc<dyn SemanticProvider>>,
    a: RelatedBlocksArgs,
) -> ToolResult {
    check_graph(r, a.graph.as_deref())?;
    let provider = provider.ok_or_else(|| failure(SemanticFailure::Disabled))?;
    let mut src = r
        .block(&a.block_uuid)?
        .ok_or_else(|| ToolError::not_found(format!("block `{}`", a.block_uuid)))?;
    strip_properties(&mut src);
    let query = cut(&src.content, QUERY_CHARS);
    if query.trim().is_empty() {
        return Err(ToolError::invalid("the block has no text to compare"));
    }
    let n = limit(a.limit, 10, 50);
    let found = provider.search(&query, n * 2 + 1).map_err(failure)?;
    let hits = resolve(r, found, Some(&src.uuid), n)?;
    render(
        &format!("Blocks related to `{}`", a.block_uuid),
        query,
        hits,
    )
}
