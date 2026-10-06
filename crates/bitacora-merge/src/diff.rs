//! Block-level diff of two versions of a page (BIT-US-0048, BIT-SP-0006.R22).
//!
//! Blocks of the two versions are paired with the same matcher the three-way merge uses
//! ([`match_pages`]: `id::` first, then content hash and fuzzy similarity), so a block that was
//! edited in place is reported as `Changed` instead of removed plus added. The diff is directed:
//! `old` is the historical version, `new` the current page, so restoring means turning `new`
//! back into `old`.

use crate::matcher::match_pages;
use crate::model::MergePage;

/// What happened to a block between `old` and `new`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    /// Only in `new`.
    Added,
    /// Only in `old`.
    Removed,
    /// Text, content properties or planning lines differ.
    Changed,
    /// Only metadata differs (`id::`, `collapsed::`, logbook, ...): hidden by default.
    MetaOnly,
}

/// One block-level difference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDiff {
    /// Kind of change.
    pub kind: DiffKind,
    /// Block index in the `old` page (document order), when present there.
    pub old: Option<usize>,
    /// Block index in the `new` page, when present there.
    pub new: Option<usize>,
    /// Content in `old`.
    pub old_text: Option<String>,
    /// Content in `new`.
    pub new_text: Option<String>,
    /// Ancestor titles of the block (from `new` when present, else from `old`).
    pub breadcrumb: Vec<String>,
}

/// The differences between two versions of a page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageDiff {
    /// Block differences ordered by position (in `new`, removed blocks next to their old
    /// neighbours).
    pub blocks: Vec<BlockDiff>,
    /// The page-level property block differs.
    pub preamble_changed: bool,
    /// Mapping `old index -> new index` of paired blocks (also the unchanged ones).
    pub pairs: Vec<Option<usize>>,
}

impl PageDiff {
    /// True when nothing differs (metadata-only differences included).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && !self.preamble_changed
    }

    /// The differences to show: `MetaOnly` ones only when `include_meta`.
    pub fn visible(&self, include_meta: bool) -> impl Iterator<Item = &BlockDiff> {
        self.blocks
            .iter()
            .filter(move |b| include_meta || b.kind != DiffKind::MetaOnly)
    }
}

type PreView<'a> = Option<(&'a str, Vec<(&'a str, &'a str)>)>;

fn pre_text(p: &MergePage) -> PreView<'_> {
    p.pre_block.as_ref().map(|pb| {
        (
            pb.text.as_str(),
            pb.props
                .iter()
                .map(|e| (e.norm.as_str(), e.value.as_str()))
                .collect(),
        )
    })
}

/// Diffs `old` against `new` at block level.
#[must_use]
pub fn diff_pages(old: &MergePage, new: &MergePage) -> PageDiff {
    let pairs = match_pages(old, new);
    let mut paired_new = vec![false; new.blocks.len()];
    // Entries are keyed by a position in `new` (a removed block sits before the next block of
    // `new` that follows its old predecessor), then by old index, so the list reads in order.
    let mut keyed: Vec<(usize, usize, BlockDiff)> = Vec::new();
    let mut anchor = 0usize;
    for (oi, ob) in old.blocks.iter().enumerate() {
        match pairs[oi] {
            Some(ni) => {
                paired_new[ni] = true;
                anchor = ni + 1;
                let nb = &new.blocks[ni];
                if ob.same_as(nb) {
                    continue;
                }
                let kind =
                    if ob.content == nb.content && ob.normalized_hash() == nb.normalized_hash() {
                        DiffKind::MetaOnly
                    } else {
                        DiffKind::Changed
                    };
                keyed.push((
                    ni,
                    oi,
                    BlockDiff {
                        kind,
                        old: Some(oi),
                        new: Some(ni),
                        old_text: Some(ob.content.clone()),
                        new_text: Some(nb.content.clone()),
                        breadcrumb: new.breadcrumb(ni),
                    },
                ));
            }
            None => keyed.push((
                anchor,
                oi,
                BlockDiff {
                    kind: DiffKind::Removed,
                    old: Some(oi),
                    new: None,
                    old_text: Some(ob.content.clone()),
                    new_text: None,
                    breadcrumb: old.breadcrumb(oi),
                },
            )),
        }
    }
    for (ni, nb) in new.blocks.iter().enumerate() {
        if !paired_new[ni] {
            keyed.push((
                ni,
                usize::MAX,
                BlockDiff {
                    kind: DiffKind::Added,
                    old: None,
                    new: Some(ni),
                    old_text: None,
                    new_text: Some(nb.content.clone()),
                    breadcrumb: new.breadcrumb(ni),
                },
            ));
        }
    }
    keyed.sort_by_key(|(pos, oi, _)| (*pos, *oi));
    PageDiff {
        blocks: keyed.into_iter().map(|(_, _, d)| d).collect(),
        preamble_changed: pre_text(old) != pre_text(new),
        pairs,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn d(old: &str, new: &str) -> PageDiff {
        diff_pages(&MergePage::parse(old), &MergePage::parse(new))
    }

    #[test]
    fn identical_pages_have_no_diff() {
        assert!(d("- a\n- b\n", "- a\n- b\n").is_empty());
    }

    #[test]
    fn edit_is_changed_not_remove_plus_add() {
        let r = d(
            "- alpha beta gamma\n- b\n",
            "- alpha beta gamma delta\n- b\n",
        );
        assert_eq!(r.blocks.len(), 1);
        assert_eq!(r.blocks[0].kind, DiffKind::Changed);
        assert_eq!(r.blocks[0].old_text.as_deref(), Some("alpha beta gamma"));
        assert_eq!(
            r.blocks[0].new_text.as_deref(),
            Some("alpha beta gamma delta")
        );
    }

    #[test]
    fn added_and_removed_blocks() {
        let r = d("- one\n- two\n- three\n", "- one\n- three\n- brand new\n");
        let kinds: Vec<DiffKind> = r.blocks.iter().map(|b| b.kind).collect();
        assert!(kinds.contains(&DiffKind::Removed), "{r:?}");
        assert!(kinds.contains(&DiffKind::Added), "{r:?}");
        let removed = r
            .blocks
            .iter()
            .find(|b| b.kind == DiffKind::Removed)
            .unwrap();
        assert_eq!(removed.old_text.as_deref(), Some("two"));
    }

    #[test]
    fn metadata_only_changes_are_hidden_by_default() {
        let r = d("- a\n- b\n", "- a\n  collapsed:: true\n- b\n");
        assert_eq!(r.blocks.len(), 1);
        assert_eq!(r.blocks[0].kind, DiffKind::MetaOnly);
        assert_eq!(r.visible(false).count(), 0);
        assert_eq!(r.visible(true).count(), 1);
    }

    #[test]
    fn nested_removed_block_keeps_breadcrumb_and_preamble_flag() {
        let r = d(
            "title:: x\n\n- parent\n  - kid\n",
            "title:: y\n\n- parent\n",
        );
        assert!(r.preamble_changed);
        let kid = r
            .blocks
            .iter()
            .find(|b| b.kind == DiffKind::Removed)
            .unwrap();
        assert_eq!(kid.breadcrumb, ["parent"]);
    }
}
