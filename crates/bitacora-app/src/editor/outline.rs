//! The editable page as the editor sees it: a snapshot of the core page with its tree
//! structure, plus the render rows derived from it. Pure data, no GPUI types.

use std::collections::HashMap;
use std::sync::Arc;

use bitacora_core::editor::{BlockId, ClipBlock};
use bitacora_core::queue::{PageSnapshot, SnapshotBlock};
use bitacora_markdown::properties::PropertyConfig;

use crate::data::{GraphHandle, IndexResolver};
use crate::render::model::{BlockModel, Row};

/// A core page snapshot with lookups.
#[derive(Debug, Clone)]
pub struct Outline {
    snap: Arc<PageSnapshot>,
    pos: HashMap<BlockId, usize>,
}

impl Outline {
    /// Indexes `snap`.
    pub fn new(snap: Arc<PageSnapshot>) -> Self {
        let pos = snap
            .blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.id, i))
            .collect();
        Self { snap, pos }
    }

    /// The snapshot.
    pub fn snapshot(&self) -> &Arc<PageSnapshot> {
        &self.snap
    }

    /// Blocks in document order.
    pub fn blocks(&self) -> &[SnapshotBlock] {
        &self.snap.blocks
    }

    /// Document-order index of `id`.
    pub fn index_of(&self, id: BlockId) -> Option<usize> {
        self.pos.get(&id).copied()
    }

    /// The block `id`.
    pub fn block(&self, id: BlockId) -> Option<&SnapshotBlock> {
        self.index_of(id).map(|i| &self.snap.blocks[i])
    }

    /// Index after the last descendant of block `ix`.
    pub fn subtree_end(&self, ix: usize) -> usize {
        let depth = self.snap.blocks[ix].depth;
        self.snap.blocks[ix + 1..]
            .iter()
            .position(|b| b.depth <= depth)
            .map_or(self.snap.blocks.len(), |n| ix + 1 + n)
    }

    /// Whether block `ix` has children.
    pub fn has_children(&self, ix: usize) -> bool {
        self.snap
            .blocks
            .get(ix + 1)
            .is_some_and(|b| b.depth > self.snap.blocks[ix].depth)
    }

    /// Ancestors of `id`, outermost first.
    pub fn ancestors(&self, id: BlockId) -> Vec<BlockId> {
        let mut out = Vec::new();
        let mut cur = self.block(id).and_then(|b| b.parent);
        while let Some(p) = cur {
            out.push(p);
            cur = self.block(p).and_then(|b| b.parent);
        }
        out.reverse();
        out
    }

    /// Whether `node` is `ancestor` or inside it.
    pub fn is_within(&self, ancestor: BlockId, node: BlockId) -> bool {
        match (self.index_of(ancestor), self.index_of(node)) {
            (Some(a), Some(n)) => n >= a && n < self.subtree_end(a),
            _ => false,
        }
    }

    /// The top-level blocks of `ids` (those without a selected ancestor), in document order.
    pub fn top_level(&self, ids: &[BlockId]) -> Vec<BlockId> {
        let mut found: Vec<usize> = ids.iter().filter_map(|id| self.index_of(*id)).collect();
        found.sort_unstable();
        found.dedup();
        let mut out = Vec::new();
        let mut covered_until = 0;
        for ix in found {
            if ix < covered_until {
                continue;
            }
            covered_until = self.subtree_end(ix);
            out.push(self.snap.blocks[ix].id);
        }
        out
    }

    /// The clipboard trees of `ids` (top-level blocks only).
    pub fn trees(&self, ids: &[BlockId]) -> Vec<ClipBlock> {
        self.top_level(ids)
            .into_iter()
            .filter_map(|id| self.index_of(id))
            .map(|ix| self.tree_at(ix).0)
            .collect()
    }

    fn tree_at(&self, ix: usize) -> (ClipBlock, usize) {
        let depth = self.snap.blocks[ix].depth;
        let mut node = ClipBlock {
            text: self.snap.blocks[ix].text.clone(),
            children: Vec::new(),
        };
        let mut next = ix + 1;
        while next < self.snap.blocks.len() && self.snap.blocks[next].depth > depth {
            let (child, after) = self.tree_at(next);
            node.children.push(child);
            next = after;
        }
        (node, next)
    }

    /// Render rows for the whole page, or for the subtree of `zoom` (the root included, shown
    /// open). The ids are parallel to the rows.
    pub fn rows(
        &self,
        zoom: Option<BlockId>,
        handle: &GraphHandle,
        cfg: &PropertyConfig,
    ) -> (Vec<Row>, Vec<BlockId>) {
        let (from, to) = match zoom.and_then(|z| self.index_of(z)) {
            Some(ix) => (ix, self.subtree_end(ix)),
            None => (0, self.snap.blocks.len()),
        };
        let base = self.snap.blocks.get(from).map_or(1, |b| b.depth);
        let resolver = IndexResolver(&handle.reader);
        let mut rows = Vec::with_capacity(to - from);
        let mut ids = Vec::with_capacity(to - from);
        for ix in from..to {
            let b = &self.snap.blocks[ix];
            let block = BlockModel::from_content(&b.text, cfg, &resolver);
            let ref_count = match &b.uuid {
                Some(u) => handle.reader.block_ref_count(&u.to_string()).unwrap_or(0),
                None => 0,
            };
            rows.push(Row {
                block_index: Some(ix),
                depth: b.depth.saturating_sub(base),
                has_children: self.has_children(ix),
                block,
                uuid: b.uuid.map(|u| u.to_string()),
                view_collapsed: (ix == from && zoom.is_some()).then_some(false),
                ref_count,
                referrers: None,
            });
            ids.push(b.id);
        }
        (rows, ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitacora_core::graph::PageKey;

    fn outline(spec: &[(usize, &str)]) -> Outline {
        let mut blocks: Vec<SnapshotBlock> = Vec::new();
        let mut stack: Vec<(usize, BlockId)> = Vec::new();
        for (n, (depth, text)) in spec.iter().enumerate() {
            let id = BlockId::from_raw(n as u64 + 1);
            while stack.last().is_some_and(|(d, _)| *d >= *depth) {
                stack.pop();
            }
            blocks.push(SnapshotBlock {
                id,
                parent: stack.last().map(|(_, i)| *i),
                depth: *depth,
                text: (*text).to_owned(),
                uuid: None,
            });
            stack.push((*depth, id));
        }
        Outline::new(Arc::new(PageSnapshot {
            key: PageKey::from_title("p"),
            title: "p".into(),
            path: None,
            preamble: None,
            blocks,
            dirty: false,
            version: 1,
        }))
    }

    fn id(n: u64) -> BlockId {
        BlockId::from_raw(n)
    }

    #[test]
    fn structure_queries() {
        let o = outline(&[(1, "a"), (2, "b"), (3, "c"), (2, "d"), (1, "e")]);
        assert_eq!(o.subtree_end(0), 4);
        assert_eq!(o.subtree_end(1), 3);
        assert!(o.has_children(0) && !o.has_children(2) && !o.has_children(4));
        assert_eq!(o.ancestors(id(3)), [id(1), id(2)]);
        assert!(o.is_within(id(1), id(3)) && !o.is_within(id(2), id(4)));
    }

    #[test]
    fn top_level_drops_descendants_and_sorts() {
        let o = outline(&[(1, "a"), (2, "b"), (2, "c"), (1, "d")]);
        assert_eq!(o.top_level(&[id(4), id(2), id(1), id(3)]), [id(1), id(4)]);
        assert_eq!(o.top_level(&[id(3), id(2)]), [id(2), id(3)]);
    }

    #[test]
    fn trees_follow_the_structure() {
        let o = outline(&[(1, "a"), (2, "b"), (3, "c"), (1, "d")]);
        let t = o.trees(&[id(1), id(2)]);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].text, "a");
        assert_eq!(t[0].children[0].children[0].text, "c");
    }
}
