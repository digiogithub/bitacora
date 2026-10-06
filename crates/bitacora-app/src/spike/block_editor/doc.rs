//! In-memory outline for the spike: a flat `Vec` of blocks with a depth each.
//! Invariant: `depth[0] == 0` and `depth[i + 1] <= depth[i] + 1`.

use std::ops::Range;

/// One block of the spike page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpikeBlock {
    /// Raw Markdown source of the block.
    pub text: String,
    /// Nesting depth (0 = top level).
    pub depth: u8,
    /// Children hidden.
    pub collapsed: bool,
}

impl SpikeBlock {
    /// A block at `depth`.
    pub fn new(text: impl Into<String>, depth: u8) -> Self {
        Self {
            text: text.into(),
            depth,
            collapsed: false,
        }
    }
}

/// The spike page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpikeDoc {
    /// Blocks in document (DFS) order.
    pub blocks: Vec<SpikeBlock>,
}

impl SpikeDoc {
    /// Builds a doc from `(depth, text)` pairs; depths are repaired to satisfy the invariant.
    pub fn from_blocks(items: impl IntoIterator<Item = (u8, String)>) -> Self {
        let mut blocks: Vec<SpikeBlock> = Vec::new();
        for (depth, text) in items {
            let max = blocks.last().map_or(0, |b| b.depth + 1);
            blocks.push(SpikeBlock::new(text, depth.min(max)));
        }
        if blocks.is_empty() {
            blocks.push(SpikeBlock::new("", 0));
        }
        Self { blocks }
    }

    /// Parses a Logseq-style outline (`- ` bullets indented with tabs or 2/4 spaces).
    /// Continuation lines are appended to the block; this is spike-grade parsing.
    pub fn from_outline(source: &str) -> Self {
        let mut items: Vec<(u8, String)> = Vec::new();
        for line in source.lines() {
            let indent_end = line.len() - line.trim_start_matches([' ', '\t']).len();
            let (indent, rest) = line.split_at(indent_end);
            if let Some(content) = rest.strip_prefix("- ").or_else(|| rest.strip_prefix('-')) {
                let tabs = indent.matches('\t').count() + indent.matches(' ').count() / 2;
                items.push((tabs.min(40) as u8, content.trim_end().to_owned()));
            } else if let Some((_, text)) = items.last_mut()
                && !rest.trim().is_empty()
            {
                text.push('\n');
                text.push_str(rest.trim_end());
            }
        }
        Self::from_blocks(items)
    }

    /// Number of blocks.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// `true` when there are no blocks.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// End (exclusive) of the subtree rooted at `ix`.
    pub fn subtree_end(&self, ix: usize) -> usize {
        let depth = self.blocks[ix].depth;
        let mut end = ix + 1;
        while end < self.blocks.len() && self.blocks[end].depth > depth {
            end += 1;
        }
        end
    }

    /// `true` if the block has at least one child.
    pub fn has_children(&self, ix: usize) -> bool {
        self.blocks
            .get(ix + 1)
            .is_some_and(|next| next.depth > self.blocks[ix].depth)
    }

    /// Indices of the visible blocks (children of collapsed blocks are skipped).
    pub fn visible_rows(&self) -> Vec<usize> {
        let mut rows = Vec::with_capacity(self.blocks.len());
        let mut ix = 0;
        while ix < self.blocks.len() {
            rows.push(ix);
            if self.blocks[ix].collapsed && self.has_children(ix) {
                ix = self.subtree_end(ix);
            } else {
                ix += 1;
            }
        }
        rows
    }

    /// Splits block `ix` at byte `at`: the head keeps `text[..at]`, the tail becomes a
    /// new block holding `text[at..]`. The new block is the first child when the block
    /// has visible children, otherwise the next sibling. Returns the new block's index.
    pub fn split(&mut self, ix: usize, at: usize) -> usize {
        let at = super::text_ops::clamp_to_boundary(&self.blocks[ix].text, at);
        let tail = self.blocks[ix].text.split_off(at);
        let depth = self.blocks[ix].depth;
        let has_visible_children = self.has_children(ix) && !self.blocks[ix].collapsed;
        let (new_ix, new_depth) = if has_visible_children {
            (ix + 1, depth + 1)
        } else {
            (self.subtree_end(ix), depth)
        };
        self.blocks.insert(new_ix, SpikeBlock::new(tail, new_depth));
        new_ix
    }

    /// Merges block `ix` into the previous visible block (Backspace at offset 0). The
    /// children of `ix` become children of the previous block. Returns
    /// `(previous_index, join_offset)`, or `None` for the first block.
    pub fn merge_with_previous(&mut self, ix: usize) -> Option<(usize, usize)> {
        let rows = self.visible_rows();
        let row = rows.iter().position(|&r| r == ix)?;
        let prev = *rows.get(row.checked_sub(1)?)?;
        let removed = self.blocks.remove(ix);
        let join = self.blocks[prev].text.len();
        self.blocks[prev].text.push_str(&removed.text);
        // Re-parent: the children of the removed block (still starting at `ix`).
        let child_depth = self.blocks[prev].depth + 1;
        let old_child_depth = removed.depth + 1;
        let mut end = ix;
        while end < self.blocks.len() && self.blocks[end].depth >= old_child_depth {
            end += 1;
        }
        for block in &mut self.blocks[ix..end] {
            // Shift by a constant so relative nesting is kept.
            let rel = block.depth - old_child_depth;
            block.depth = child_depth + rel;
        }
        Some((prev, join))
    }

    /// Whether `indent` would be valid for `ix`.
    pub fn can_indent(&self, ix: usize) -> bool {
        ix > 0 && self.blocks[ix - 1].depth >= self.blocks[ix].depth
    }

    /// Tab: nests the block (and its subtree) one level deeper. Returns `false` if impossible.
    pub fn indent(&mut self, ix: usize) -> bool {
        if !self.can_indent(ix) {
            return false;
        }
        let end = self.subtree_end(ix);
        for block in &mut self.blocks[ix..end] {
            block.depth += 1;
        }
        // The new parent must be expanded so the block stays visible.
        let depth = self.blocks[ix].depth;
        if let Some(parent) = (0..ix).rev().find(|&p| self.blocks[p].depth + 1 == depth) {
            self.blocks[parent].collapsed = false;
        }
        true
    }

    /// Shift-Tab: un-nests the block and its subtree one level. Following siblings
    /// become its children (Logseq's default, non-"logical" outdenting).
    pub fn outdent(&mut self, ix: usize) -> bool {
        if self.blocks[ix].depth == 0 {
            return false;
        }
        let end = self.subtree_end(ix);
        for block in &mut self.blocks[ix..end] {
            block.depth -= 1;
        }
        true
    }

    /// Sets the collapsed flag of `ix` (no-op without children). Returns `true` on change.
    pub fn set_collapsed(&mut self, ix: usize, collapsed: bool) -> bool {
        if !self.has_children(ix) || self.blocks[ix].collapsed == collapsed {
            return false;
        }
        self.blocks[ix].collapsed = collapsed;
        true
    }

    /// Checks the depth invariant.
    pub fn is_valid(&self) -> bool {
        self.blocks.first().is_some_and(|b| b.depth == 0)
            && self.blocks.windows(2).all(|w| w[1].depth <= w[0].depth + 1)
    }

    /// Descendant range of `ix` (excluding the block itself).
    pub fn descendants(&self, ix: usize) -> Range<usize> {
        ix + 1..self.subtree_end(ix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(items: &[(u8, &str)]) -> SpikeDoc {
        SpikeDoc::from_blocks(items.iter().map(|(d, t)| (*d, (*t).to_owned())))
    }

    fn shape(doc: &SpikeDoc) -> Vec<(u8, &str)> {
        doc.blocks
            .iter()
            .map(|b| (b.depth, b.text.as_str()))
            .collect()
    }

    #[test]
    fn split_makes_sibling_or_first_child() {
        let mut d = doc(&[(0, "alpha"), (1, "child"), (0, "beta")]);
        let new = d.split(2, 2);
        assert_eq!(new, 3);
        assert_eq!(shape(&d)[2..], [(0, "be"), (0, "ta")]);
        // Block with visible children: the tail becomes the first child.
        let new = d.split(0, 1);
        assert_eq!(new, 1);
        assert_eq!(shape(&d)[..3], [(0, "a"), (1, "lpha"), (1, "child")]);
        assert!(d.is_valid());
    }

    #[test]
    fn split_of_collapsed_block_inserts_after_the_subtree() {
        let mut d = doc(&[(0, "a"), (1, "hidden"), (0, "b")]);
        d.blocks[0].collapsed = true;
        let new = d.split(0, 1);
        assert_eq!(new, 2);
        assert_eq!(shape(&d), [(0, "a"), (1, "hidden"), (0, ""), (0, "b")]);
    }

    #[test]
    fn merge_joins_text_and_reparents_children() {
        let mut d = doc(&[(0, "one"), (0, "two"), (1, "kid"), (2, "grandkid")]);
        let (prev, join) = d.merge_with_previous(1).expect("merge");
        assert_eq!((prev, join), (0, 3));
        assert_eq!(shape(&d), [(0, "onetwo"), (1, "kid"), (2, "grandkid")]);
        assert!(d.is_valid());
        assert!(d.merge_with_previous(0).is_none());
    }

    #[test]
    fn merge_into_previous_deep_block_keeps_valid_depths() {
        let mut d = doc(&[(0, "p"), (1, "deep"), (0, "next"), (1, "kid")]);
        let (prev, join) = d.merge_with_previous(2).expect("merge");
        assert_eq!((prev, join), (1, 4));
        assert_eq!(shape(&d), [(0, "p"), (1, "deepnext"), (2, "kid")]);
        assert!(d.is_valid());
    }

    #[test]
    fn indent_requires_a_previous_sibling_and_moves_the_subtree() {
        let mut d = doc(&[(0, "a"), (0, "b"), (1, "b1")]);
        assert!(!d.indent(0));
        assert!(d.indent(1));
        assert_eq!(shape(&d), [(0, "a"), (1, "b"), (2, "b1")]);
        assert!(!d.indent(1));
        assert!(d.is_valid());
    }

    #[test]
    fn outdent_adopts_following_siblings() {
        let mut d = doc(&[(0, "p"), (1, "x"), (1, "y"), (0, "q")]);
        assert!(d.outdent(1));
        assert_eq!(shape(&d), [(0, "p"), (0, "x"), (1, "y"), (0, "q")]);
        assert!(!d.outdent(1));
        assert!(d.is_valid());
    }

    #[test]
    fn collapsed_blocks_hide_their_subtree_from_visible_rows() {
        let mut d = doc(&[(0, "a"), (1, "a1"), (2, "a11"), (0, "b")]);
        assert_eq!(d.visible_rows(), [0, 1, 2, 3]);
        assert!(d.set_collapsed(0, true));
        assert_eq!(d.visible_rows(), [0, 3]);
        assert!(
            !d.set_collapsed(3, true),
            "no children, nothing to collapse"
        );
    }

    #[test]
    fn from_blocks_repairs_depths_and_outline_parses_tabs() {
        let d = doc(&[(3, "x"), (5, "y")]);
        assert_eq!(shape(&d), [(0, "x"), (1, "y")]);
        let d = SpikeDoc::from_outline("- a\n\t- b\n\t  more\n- c\n");
        assert_eq!(shape(&d), [(0, "a"), (1, "b\nmore"), (0, "c")]);
    }
}
