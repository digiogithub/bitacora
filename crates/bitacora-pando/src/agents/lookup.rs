//! The index reads that review and recommendation results are validated against.
//!
//! An agent's structured output names blocks by uuid. Before anything reaches the user it is
//! cross-checked with the index (BIT-T-0461, BIT-T-0464): blocks that do not exist, pages the
//! guard hides and spans that no longer match the text are dropped. [`BlockLookup`] is the seam;
//! [`IndexReader`] implements it for production and [`StaticLookup`] serves tests.

use std::collections::BTreeMap;

use bitacora_index::IndexReader;

use super::AgentError;
use super::guard::AttachedBlock;

/// A block as the index currently holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockInfo {
    /// Block data (page, path, tags, uuid, text, privacy flag).
    pub block: AttachedBlock,
    /// Task marker (`TODO`, `DONE`, ...).
    pub marker: Option<String>,
    /// `yyyyMMdd` of a journal page.
    pub journal_day: Option<i64>,
}

impl BlockInfo {
    /// A task that is still open (`TODO`, `DOING`, `NOW`, `LATER`, `WAITING`).
    #[must_use]
    pub fn is_open_task(&self) -> bool {
        matches!(
            self.marker
                .as_deref()
                .map(str::to_ascii_uppercase)
                .as_deref(),
            Some("TODO" | "DOING" | "NOW" | "LATER" | "WAITING")
        )
    }
}

/// Reads of the current index state.
pub trait BlockLookup: Send + Sync {
    /// The block with `uuid`, `None` when unknown.
    ///
    /// # Errors
    /// [`AgentError::Index`] when the index cannot be read.
    fn block(&self, uuid: &str) -> Result<Option<BlockInfo>, AgentError>;

    /// Every block of the journal pages whose day (`yyyyMMdd`) is within `from..=to`.
    ///
    /// # Errors
    /// [`AgentError::Index`] when the index cannot be read.
    fn journal_blocks(&self, from_day: i64, to_day: i64) -> Result<Vec<BlockInfo>, AgentError>;
}

fn info(b: &bitacora_index::SemanticBlock) -> BlockInfo {
    let private = b.page_properties.iter().any(|(k, v)| {
        k.trim().eq_ignore_ascii_case("private") && v.trim().eq_ignore_ascii_case("true")
    });
    BlockInfo {
        block: AttachedBlock {
            page: b.page_title.clone(),
            file_path: b.file_path.clone(),
            tags: b.tags.clone(),
            uuid: Some(b.uuid.clone()),
            text: b.content.clone(),
            page_private: private,
        },
        marker: b.marker.clone(),
        journal_day: b.journal_day,
    }
}

impl BlockLookup for IndexReader {
    fn block(&self, uuid: &str) -> Result<Option<BlockInfo>, AgentError> {
        self.semantic_block(uuid)
            .map(|b| b.as_ref().map(info))
            .map_err(|e| AgentError::Index(e.to_string()))
    }

    fn journal_blocks(&self, from_day: i64, to_day: i64) -> Result<Vec<BlockInfo>, AgentError> {
        let err = |e: bitacora_index::Error| AgentError::Index(e.to_string());
        let pages = self
            .journals(Some(to_day.saturating_add(1)), 800)
            .map_err(err)?;
        let mut out = Vec::new();
        for p in pages {
            let (Some(day), Some(path)) = (p.journal_day, p.file_path.as_deref()) else {
                continue;
            };
            if day < from_day || day > to_day {
                continue;
            }
            for b in self.semantic_blocks(path).map_err(err)? {
                if !b.is_pre_block {
                    out.push(info(&b));
                }
            }
        }
        Ok(out)
    }
}

/// An in-memory [`BlockLookup`] (tests, previews).
#[derive(Debug, Clone, Default)]
pub struct StaticLookup {
    blocks: BTreeMap<String, BlockInfo>,
}

impl StaticLookup {
    /// An empty lookup.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a block (keyed by its uuid).
    #[must_use]
    pub fn with(mut self, info: BlockInfo) -> Self {
        if let Some(u) = info.block.uuid.clone() {
            self.blocks.insert(u, info);
        }
        self
    }
}

impl BlockLookup for StaticLookup {
    fn block(&self, uuid: &str) -> Result<Option<BlockInfo>, AgentError> {
        Ok(self.blocks.get(uuid).cloned())
    }

    fn journal_blocks(&self, from_day: i64, to_day: i64) -> Result<Vec<BlockInfo>, AgentError> {
        Ok(self
            .blocks
            .values()
            .filter(|b| b.journal_day.is_some_and(|d| d >= from_day && d <= to_day))
            .cloned()
            .collect())
    }
}
