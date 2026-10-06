//! Editable page model: a block tree with session-stable [`BlockId`]s over a byte-preserving
//! [`Document`] (ADR-003, ADR-006; `docs/design/block-editor.md` §2).
//!
//! Every block remembers the exact bytes it was loaded from ([`Origin`]). A block is *clean* when
//! its text and depth equal what was loaded; clean blocks are written back verbatim, everything
//! else goes through the canonical writer of `bitacora-markdown`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bitacora_markdown::edit::identity::block_id;
use bitacora_markdown::{Document, Node, RawBlock, WriteOptions, serialize};
use uuid::Uuid;

use crate::graph::PageKey;
use crate::graph_path::GraphPath;

/// Session-local block identity. Stable across edits and moves (also across pages) and never
/// persisted (ADR-006: only referenced blocks get an `id::` line in the file).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(u64);

impl BlockId {
    /// Builds an id from its raw value (tests and snapshots).
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Allocator of [`BlockId`]s, shared by every page of a workspace. Thread-safe so pure planners
/// can allocate ids for new blocks while holding only `&Workspace`.
#[derive(Debug, Default)]
pub struct IdGen(AtomicU64);

impl IdGen {
    /// Next fresh id.
    pub fn next(&self) -> BlockId {
        BlockId(self.0.fetch_add(1, Ordering::Relaxed) + 1)
    }
}

/// 64-bit content hash used for the cheap "is this text still what was loaded" test.
#[must_use]
pub fn text_hash(text: &str) -> u64 {
    let h = blake3::hash(text.as_bytes());
    let b = h.as_bytes();
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

/// Where a block's own bytes came from. Only valid for the page and byte generation it was
/// loaded from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    /// Page whose bytes the span refers to.
    pub page: PageKey,
    /// Byte generation of that page (bumped whenever its base bytes are replaced).
    pub generation: u64,
    /// The raw block (span, levels) in the base bytes.
    pub raw: RawBlock,
    /// Tree depth (1-based) when loaded.
    pub depth: usize,
    /// [`text_hash`] of the block text when loaded.
    pub text_hash: u64,
}

/// One block of a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Session-stable id.
    pub id: BlockId,
    /// Parent block, `None` for top-level blocks.
    pub parent: Option<BlockId>,
    /// Children in order.
    pub children: Vec<BlockId>,
    /// Logical text as Logseq's `:block/content`: no bullet, no continuation indentation,
    /// property lines (including `id::` and `collapsed::`) kept.
    pub text: String,
    /// The `id::` of the text, when it has a valid one.
    pub uuid: Option<Uuid>,
    /// Loaded bytes; `None` for blocks created in this session.
    pub origin: Option<Origin>,
}

impl Block {
    /// Recomputes `uuid` from `text`.
    pub(crate) fn refresh_uuid(&mut self) {
        self.uuid = block_id(&self.text);
    }
}

/// A detached subtree (ids, texts and origins are kept so that an inverse restores it exactly).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subtree {
    /// Id of the subtree root.
    pub id: BlockId,
    /// Text of the root.
    pub text: String,
    /// Origin of the root.
    pub origin: Option<Origin>,
    /// Child subtrees in order.
    pub children: Vec<Subtree>,
}

impl Subtree {
    /// A new leaf block without origin.
    #[must_use]
    pub fn new(id: BlockId, text: impl Into<String>) -> Self {
        Self {
            id,
            text: text.into(),
            origin: None,
            children: Vec::new(),
        }
    }

    /// Adds children.
    #[must_use]
    pub fn with_children(mut self, children: Vec<Subtree>) -> Self {
        self.children = children;
        self
    }

    /// Ids of the whole subtree, root first.
    #[must_use]
    pub fn ids(&self) -> Vec<BlockId> {
        let mut out = vec![self.id];
        for c in &self.children {
            out.extend(c.ids());
        }
        out
    }
}

/// A slot in a page's tree. `index` counts the siblings **without** the block being placed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    /// Page.
    pub page: PageKey,
    /// Parent block, `None` for the page root.
    pub parent: Option<BlockId>,
    /// Index among the siblings.
    pub index: usize,
}

/// What we believe is on disk for a page (the merge base, in memory only, ADR-017).
#[derive(Debug, Clone)]
pub struct DiskSnapshot {
    /// Last bytes read from or written to disk.
    pub bytes: Arc<[u8]>,
    /// BLAKE3 of `bytes`.
    pub hash: blake3::Hash,
}

impl DiskSnapshot {
    /// Snapshot of `bytes`.
    #[must_use]
    pub fn new(bytes: &[u8]) -> Self {
        Self {
            bytes: Arc::from(bytes),
            hash: blake3::hash(bytes),
        }
    }
}

/// Errors of page-level operations.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// The freshly written bytes no longer match the model structure.
    #[error("saved bytes do not match the page model")]
    SnapshotMismatch,
}

/// An editable page.
#[derive(Debug, Clone)]
pub struct Page {
    /// Page identity.
    pub key: PageKey,
    /// Display title.
    pub title: String,
    /// Where the page should live; `None` for a virtual page (no file yet).
    pub path: Option<GraphPath>,
    /// Where the bytes currently are on disk (differs from `path` after a file rename).
    pub disk_path: Option<GraphPath>,
    /// Pre-block text (front matter / page properties), if any.
    pub preamble: Option<String>,
    /// Top-level blocks in order.
    pub roots: Vec<BlockId>,
    /// All blocks of the page.
    pub blocks: HashMap<BlockId, Block>,
    /// Last bytes read from or written to disk; `None` for a page never written.
    pub disk: Option<DiskSnapshot>,
    /// Content differs from `disk`.
    pub dirty: bool,
    /// Never written by Bitacora (`.org`).
    pub read_only: bool,
    pre_origin: Option<(Node, u64, u64)>,
    base: Document,
    generation: u64,
}

impl Page {
    /// An empty page (no bytes yet).
    #[must_use]
    pub fn empty(key: PageKey, title: impl Into<String>, path: Option<GraphPath>) -> Self {
        Self {
            key,
            title: title.into(),
            path,
            disk_path: None,
            preamble: None,
            roots: Vec::new(),
            blocks: HashMap::new(),
            disk: None,
            dirty: false,
            read_only: false,
            pre_origin: None,
            base: Document::parse(Vec::new()),
            generation: 0,
        }
    }

    /// Loads a page from bytes. Never fails: any byte string is a page.
    #[must_use]
    pub fn load(
        key: PageKey,
        title: impl Into<String>,
        path: Option<GraphPath>,
        bytes: &[u8],
        ids: &IdGen,
    ) -> Self {
        let mut page = Self::empty(key, title, path.clone());
        page.disk_path = path;
        page.disk = Some(DiskSnapshot::new(bytes));
        let mut doc = Document::parse(bytes.to_vec());
        page.attach_from_document(&mut doc, Some(ids));
        page
    }

    /// Fills the tree from `doc` (allocating ids when `ids` is given) or re-assigns origins from
    /// it (when `ids` is `None`, in document order, requires an identical structure).
    fn attach_from_document(&mut self, doc: &mut Document, ids: Option<&IdGen>) {
        self.generation += 1;
        let generation = self.generation;
        self.preamble = doc.pre_block_text().map(std::borrow::Cow::into_owned);
        self.pre_origin = match (&doc.pre_block, &self.preamble) {
            (Some(node), Some(t)) => Some((node.clone(), text_hash(t), generation)),
            _ => None,
        };
        let order = self.dfs();
        let mut stack: Vec<BlockId> = Vec::new();
        for (i, node) in doc.blocks.iter().enumerate() {
            let Node::Original { block, depth } = node else {
                continue;
            };
            let text = doc
                .block_content(i)
                .map(std::borrow::Cow::into_owned)
                .unwrap_or_default();
            let origin = Origin {
                page: self.key.clone(),
                generation,
                raw: block.clone(),
                depth: *depth,
                text_hash: text_hash(&text),
            };
            if let Some(ids) = ids {
                stack.truncate(depth.saturating_sub(1));
                let id = ids.next();
                let parent = stack.last().copied();
                let mut b = Block {
                    id,
                    parent,
                    children: Vec::new(),
                    text,
                    uuid: None,
                    origin: Some(origin),
                };
                b.refresh_uuid();
                self.blocks.insert(id, b);
                match parent {
                    Some(p) => {
                        if let Some(pb) = self.blocks.get_mut(&p) {
                            pb.children.push(id);
                        }
                    }
                    None => self.roots.push(id),
                }
                stack.push(id);
            } else if let Some(b) = order.get(i).and_then(|id| self.blocks.get_mut(id)) {
                b.origin = Some(origin);
            }
        }
        doc.blocks.clear();
        doc.pre_block = None;
        self.base = doc.clone();
    }

    /// Block lookup.
    #[must_use]
    pub fn block(&self, id: BlockId) -> Option<&Block> {
        self.blocks.get(&id)
    }

    /// Children of `parent` (`None` = page roots).
    #[must_use]
    pub fn children_of(&self, parent: Option<BlockId>) -> Option<&Vec<BlockId>> {
        match parent {
            None => Some(&self.roots),
            Some(p) => self.blocks.get(&p).map(|b| &b.children),
        }
    }

    /// 1-based depth of a block (0 when unknown).
    #[must_use]
    pub fn depth_of(&self, id: BlockId) -> usize {
        let mut d = 0;
        let mut cur = Some(id);
        while let Some(c) = cur {
            let Some(b) = self.blocks.get(&c) else {
                return 0;
            };
            d += 1;
            cur = b.parent;
            if d > self.blocks.len() + 1 {
                return 0;
            }
        }
        d
    }

    /// Pre-order traversal of the whole tree.
    #[must_use]
    pub fn dfs(&self) -> Vec<BlockId> {
        let mut out = Vec::with_capacity(self.blocks.len());
        let mut stack: Vec<BlockId> = self.roots.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            if out.len() > self.blocks.len() {
                break; // cycle guard; the invariant checker reports it
            }
            out.push(id);
            if let Some(b) = self.blocks.get(&id) {
                stack.extend(b.children.iter().rev().copied());
            }
        }
        out
    }

    /// True when the block would be written back verbatim: same page bytes, same text, same depth.
    /// Sibling order and parent identity do not matter, so same-depth moves keep blocks clean.
    #[must_use]
    pub fn is_clean(&self, id: BlockId) -> bool {
        let Some(b) = self.blocks.get(&id) else {
            return false;
        };
        let Some(o) = &b.origin else { return false };
        o.page == self.key
            && o.generation == self.generation
            && o.text_hash == text_hash(&b.text)
            && o.depth == self.depth_of(id)
    }

    fn pre_node(&self) -> Option<Node> {
        let text = self.preamble.as_ref()?;
        match &self.pre_origin {
            Some((node, h, g)) if *g == self.generation && *h == text_hash(text) => {
                Some(node.clone())
            }
            _ => Some(Node::Edited {
                depth: 0,
                content: text.clone(),
            }),
        }
    }

    fn nodes(&self, order: &[BlockId], canonical: bool) -> Vec<Node> {
        order
            .iter()
            .filter_map(|id| {
                let b = self.blocks.get(id)?;
                let depth = self.depth_of(*id);
                if !canonical
                    && self.is_clean(*id)
                    && let Some(o) = &b.origin
                {
                    return Some(Node::Original {
                        block: o.raw.clone(),
                        depth,
                    });
                }
                Some(Node::Edited {
                    depth,
                    content: b.text.clone(),
                })
            })
            .collect()
    }

    /// Serializes the page: untouched blocks verbatim, edited ones canonical. The output is
    /// re-parsed and compared with the model; on a mismatch (e.g. tolerant indentation of kept
    /// blocks next to rewritten ones) the whole page is rendered canonically instead, so what
    /// we write never re-parses differently.
    #[must_use]
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = self.serialize_inner();
        // A page that never existed on disk ends with a line break, like Logseq's files.
        if self.disk.is_none() && !out.is_empty() && !out.ends_with(b"\n") {
            out.push(b'\n');
        }
        out
    }

    fn serialize_inner(&self) -> Vec<u8> {
        let order = self.dfs();
        let mut doc = self.base.clone();
        doc.pre_block = self.pre_node();
        doc.blocks = self.nodes(&order, false);
        let out = serialize(&doc, &WriteOptions::default());
        if self.matches_model(&out, &order) {
            return out;
        }
        doc.pre_block = self.preamble.as_ref().map(|t| Node::Edited {
            depth: 0,
            content: t.clone(),
        });
        doc.blocks = self.nodes(&order, true);
        serialize(&doc, &WriteOptions::default())
    }

    fn matches_model(&self, bytes: &[u8], order: &[BlockId]) -> bool {
        let parsed = Document::parse(bytes.to_vec());
        if parsed.blocks.len() != order.len() {
            return false;
        }
        let pre_ok = match (&self.preamble, parsed.pre_block_text()) {
            (None, None) => true,
            (Some(a), Some(b)) => a.trim() == b.trim(),
            (Some(a), None) => a.trim().is_empty(),
            (None, Some(_)) => false,
        };
        pre_ok
            && order.iter().enumerate().all(|(i, id)| {
                self.blocks.get(id).is_some_and(|b| {
                    parsed.blocks[i].depth() == self.depth_of(*id)
                        && parsed
                            .block_content(i)
                            .is_some_and(|c| c.trim() == b.text.trim())
                })
            })
    }

    /// True when the page has to be written (content changed or file renamed).
    #[must_use]
    pub fn needs_write(&self) -> bool {
        !self.read_only && (self.dirty || (self.path.is_some() && self.path != self.disk_path))
    }

    /// Records that `bytes` (the output of [`Page::serialize`]) are now on disk: replaces the merge
    /// base, rebases every origin onto the new bytes and clears `dirty`.
    ///
    /// # Errors
    /// [`ModelError::SnapshotMismatch`] when `bytes` does not parse to the model structure.
    pub fn mark_saved(&mut self, bytes: &[u8]) -> Result<(), ModelError> {
        let mut doc = Document::parse(bytes.to_vec());
        let order = self.dfs();
        if doc.blocks.len() != order.len() {
            return Err(ModelError::SnapshotMismatch);
        }
        self.attach_from_document(&mut doc, None);
        self.disk = Some(DiskSnapshot::new(bytes));
        self.disk_path.clone_from(&self.path);
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn load(src: &str) -> Page {
        Page::load(
            PageKey::from_title("p"),
            "p",
            GraphPath::new("pages/p.md").ok(),
            src.as_bytes(),
            &IdGen::default(),
        )
    }

    #[test]
    fn load_builds_tree_and_roundtrips() {
        let src = "title:: x\n\n- a\n\t- b\n\t\t- c\n\t- d\n- e\n";
        let p = load(src);
        assert_eq!(p.roots.len(), 2);
        assert_eq!(p.blocks.len(), 5);
        assert_eq!(p.preamble.as_deref(), Some("title:: x"));
        assert!(p.dfs().iter().all(|id| p.is_clean(*id)));
        assert_eq!(p.serialize(), src.as_bytes());
        let depths: Vec<usize> = p.dfs().iter().map(|i| p.depth_of(*i)).collect();
        assert_eq!(depths, vec![1, 2, 3, 2, 1]);
    }

    #[test]
    fn mixed_styles_roundtrip() {
        for src in [
            "",
            "text only",
            "\u{feff}- a\r\n  - b\r\n",
            "- a\n\n\n- b",
            "* a\n    * b\n",
        ] {
            assert_eq!(load(src).serialize(), src.as_bytes(), "{src:?}");
        }
    }
}
