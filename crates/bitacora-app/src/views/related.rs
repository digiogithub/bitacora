//! "Related blocks" of the right panel Context tab (BIT-SP-0010.R5): blocks elsewhere in the graph
//! that Pando's semantic search finds close to the page on screen.
//!
//! The query is the page title plus its first blocks' text; the answer is the semantic half of
//! [`HybridSearch`] (the same logic the MCP `related_blocks` tool uses through the runtime's
//! adapter), minus blocks of the page itself. Everything here is blocking: call it from a
//! background thread.

use bitacora_runtime::{
    HybridOptions, HybridResults, HybridSearch, HybridTarget, SemanticState, Unavailable,
};
use rust_i18n::t;

use crate::data::GraphHandle;

/// Blocks listed.
pub const RELATED_LIMIT: usize = 8;
/// Page blocks read to build the query.
const QUERY_BLOCKS: usize = 6;
/// Longest query, in characters.
const QUERY_CHARS: usize = 400;
/// Longest row text, in characters.
const TEXT_CHARS: usize = 140;

/// One related block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedBlock {
    /// Block UUID (navigation target).
    pub uuid: String,
    /// Title of its page.
    pub page: String,
    /// One-line text.
    pub text: String,
    /// Changed locally since Pando embedded it.
    pub stale: bool,
}

/// What the section shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Related {
    /// Nothing to show: no page, or semantic search is off for this graph (section hidden).
    #[default]
    Idle,
    /// A query is running.
    Loading,
    /// The related blocks (possibly none).
    Ready(Vec<RelatedBlock>),
    /// Pando did not contribute; the text is the localized reason.
    Unavailable(String),
}

/// The query for page `name`: its title followed by the start of its content. `None` when the
/// page has no text to compare.
pub fn page_query(handle: &GraphHandle, name: &str) -> Result<Option<(String, String)>, String> {
    let Some(page) = handle
        .reader
        .page_by_name(name)
        .map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let blocks = handle
        .reader
        .outline(page.id, 0, QUERY_BLOCKS + 1, false)
        .map_err(|e| e.to_string())?;
    let mut query = page.original_name.clone();
    for block in blocks.iter().filter(|b| !b.is_pre_block).take(QUERY_BLOCKS) {
        let text = block.title.trim();
        if !text.is_empty() {
            query.push(' ');
            query.push_str(text);
        }
    }
    let query: String = query.chars().take(QUERY_CHARS).collect();
    Ok(Some((query, page.original_name)))
}

/// Keeps the semantic block hits of `results` that are not on page `own_page`, best first.
pub fn related_from(results: HybridResults, own_page: &str) -> Related {
    match results.semantic {
        // Pando is off for this graph: the section stays hidden.
        SemanticState::Unavailable(Unavailable::Disabled) => Related::Idle,
        SemanticState::Unavailable(_) => {
            let hint = super::palette::hits_from_hybrid(HybridResults {
                hits: Vec::new(),
                semantic: results.semantic,
            })
            .1
            .unwrap_or_default();
            Related::Unavailable(hint)
        }
        SemanticState::Used { .. } => {
            let mut hits: Vec<_> = results
                .hits
                .into_iter()
                .filter_map(|h| match (h.target, h.semantic_rank) {
                    (HybridTarget::Block { uuid }, Some(rank))
                        if !h.title.eq_ignore_ascii_case(own_page) =>
                    {
                        Some((
                            rank,
                            RelatedBlock {
                                uuid,
                                page: h.title,
                                text: one_line(&h.snippet.text),
                                stale: h.stale,
                            },
                        ))
                    }
                    _ => None,
                })
                .collect();
            hits.sort_by_key(|(rank, _)| *rank);
            Related::Ready(
                hits.into_iter()
                    .map(|(_, b)| b)
                    .take(RELATED_LIMIT)
                    .collect(),
            )
        }
    }
}

fn one_line(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let line = line.trim();
    if line.chars().count() > TEXT_CHARS {
        let cut: String = line.chars().take(TEXT_CHARS).collect();
        format!("{cut}...")
    } else {
        line.to_owned()
    }
}

/// Finds the blocks related to page `name`. Blocking (up to the Pando timeout).
pub fn load_related(
    handle: &GraphHandle,
    hybrid: &HybridSearch,
    name: &str,
) -> Result<Related, String> {
    let Some((query, own)) = page_query(handle, name)? else {
        return Ok(Related::Ready(Vec::new()));
    };
    let opts = HybridOptions {
        // Over-fetch: the page's own blocks are dropped afterwards.
        limit: RELATED_LIMIT * 3,
        ..HybridOptions::default()
    };
    let results = hybrid.search(&query, &opts).map_err(|e| e.to_string())?;
    Ok(related_from(results, &own))
}

/// Localized title of the section's empty state.
pub fn empty_text() -> String {
    t!("right.no_related").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitacora_index::search::Snippet;
    use bitacora_runtime::HybridHit;

    fn hit(uuid: &str, page: &str, semantic_rank: Option<usize>) -> HybridHit {
        HybridHit {
            target: HybridTarget::Block { uuid: uuid.into() },
            title: page.into(),
            is_journal: false,
            snippet: Snippet {
                text: format!("text of {uuid}\nsecond line"),
                highlights: Vec::new(),
            },
            score: 1.0,
            lexical_rank: None,
            semantic_rank,
            stale: false,
        }
    }

    #[test]
    fn keeps_semantic_blocks_of_other_pages_in_rank_order() {
        let results = HybridResults {
            hits: vec![
                hit("a", "Other", Some(2)),
                hit("b", "Own", Some(1)),
                hit("c", "Other", None),
                hit("d", "Third", Some(1)),
            ],
            semantic: SemanticState::Used {
                candidates: 4,
                dropped: 0,
            },
        };
        let Related::Ready(blocks) = related_from(results, "own") else {
            panic!("expected blocks");
        };
        let uuids: Vec<_> = blocks.iter().map(|b| b.uuid.as_str()).collect();
        assert_eq!(uuids, ["d", "a"]);
        assert_eq!(blocks[0].text, "text of d");
    }

    #[test]
    fn unavailable_semantic_gives_the_reason() {
        let results = HybridResults {
            hits: vec![hit("a", "Other", None)],
            semantic: SemanticState::Unavailable(Unavailable::Failed("boom".into())),
        };
        match related_from(results, "Own") {
            Related::Unavailable(why) => assert!(why.contains("boom"), "{why}"),
            other => panic!("{other:?}"),
        }
    }
}
