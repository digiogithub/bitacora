//! Structural merge: which blocks survive, under which parent and in which order
//! (`docs/design/git-sync-merge.md` §4.3, "Structure-level cases").
//!
//! Input is the [`Matching`] triples. Output is a [`Plan`]: the surviving triples in document
//! order with their output depth, plus the delete-vs-modify conflicts and info notes. Nothing here
//! looks at block text beyond "was it modified".

use crate::conflict::{Conflict, ConflictKind, Note, NoteKind, PageConflict};
use crate::matcher::{Matching, Triple};
use crate::model::{MergeBlock, MergePage};

/// Which side a surviving block is taken from when only one side has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fate {
    /// Present in the output.
    Keep,
    /// Removed.
    Delete,
}

/// One block of the merged outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedBlock {
    /// Index into [`Matching::triples`].
    pub triple: usize,
    /// 1-based output depth.
    pub depth: usize,
}

/// The merged outline.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// Surviving blocks in output (preorder) order.
    pub order: Vec<PlannedBlock>,
    /// Delete-vs-modify conflicts (the modified block survives).
    pub conflicts: Vec<PageConflict>,
    /// Informational notes.
    pub notes: Vec<Note>,
}

/// The three pages with the block-to-triple maps.
struct Ctx<'a> {
    pages: [&'a MergePage; 3], // base, ours, theirs
    triples: &'a [Triple],
    tri_of: [Vec<usize>; 3],
}

const B: usize = 0;
const O: usize = 1;
const T: usize = 2;

impl Ctx<'_> {
    fn idx(&self, tri: usize, side: usize) -> Option<usize> {
        let t = &self.triples[tri];
        [t.base, t.ours, t.theirs][side]
    }

    fn block(&self, tri: usize, side: usize) -> Option<&MergeBlock> {
        self.idx(tri, side).map(|i| &self.pages[side].blocks[i])
    }

    /// Parent triple of `tri` on `side`: `None` when absent there, `Some(None)` at top level.
    fn parent(&self, tri: usize, side: usize) -> Option<Option<usize>> {
        let b = self.block(tri, side)?;
        Some(b.parent.map(|p| self.tri_of[side][p]))
    }

    /// Titles of the ancestors of `tri` followed by its own first line, from the best side.
    fn breadcrumb(&self, tri: usize) -> Vec<String> {
        for side in [O, T, B] {
            if let Some(i) = self.idx(tri, side) {
                let mut v = self.pages[side].breadcrumb(i);
                v.push(self.pages[side].blocks[i].first_line().to_owned());
                return v;
            }
        }
        Vec::new()
    }

    /// Children of `parent` (`None` = top level) on `side`, as triples.
    fn children(&self, parent: Option<usize>, side: usize) -> Vec<usize> {
        let page = self.pages[side];
        let list: &[usize] = match parent {
            None => &page.roots,
            Some(p) => match self.idx(p, side) {
                Some(i) => &page.blocks[i].children,
                None => return Vec::new(),
            },
        };
        list.iter().map(|&i| self.tri_of[side][i]).collect()
    }
}

/// True when the text of the block (content, content properties, planning) differs. Metadata is
/// not a modification: it never keeps a deleted block alive.
fn modified(a: &MergeBlock, b: &MergeBlock) -> bool {
    a.content != b.content || a.normalized_hash() != b.normalized_hash()
}

/// Computes the merged outline.
#[must_use]
pub fn plan(base: &MergePage, ours: &MergePage, theirs: &MergePage, matching: &Matching) -> Plan {
    let n = matching.triples.len();
    let mut tri_of = [
        vec![usize::MAX; base.blocks.len()],
        vec![usize::MAX; ours.blocks.len()],
        vec![usize::MAX; theirs.blocks.len()],
    ];
    for (k, t) in matching.triples.iter().enumerate() {
        for (side, idx) in [t.base, t.ours, t.theirs].into_iter().enumerate() {
            if let Some(i) = idx {
                tri_of[side][i] = k;
            }
        }
    }
    let cx = Ctx {
        pages: [base, ours, theirs],
        triples: &matching.triples,
        tri_of,
    };
    let mut out = Plan::default();

    // 1. Fate of every triple.
    let mut fate = vec![Fate::Delete; n];
    for (k, f) in fate.iter_mut().enumerate() {
        let (b, o, t) = (cx.block(k, B), cx.block(k, O), cx.block(k, T));
        *f = match (b, o, t) {
            (_, Some(_), Some(_)) | (None, Some(_), None) | (None, None, Some(_)) => Fate::Keep,
            (_, None, None) => Fate::Delete,
            (Some(b), Some(survivor), None) | (Some(b), None, Some(survivor)) => {
                if modified(b, survivor) {
                    let ours_survives = o.is_some();
                    out.conflicts.push(PageConflict {
                        block_key: block_key(survivor),
                        breadcrumb: cx.breadcrumb(k),
                        conflict: Conflict {
                            kind: ConflictKind::DeleteVsModify,
                            field: "block".to_owned(),
                            base: Some(b.content.clone()),
                            ours: o.map(|x| x.content.clone()),
                            theirs: t.map(|x| x.content.clone()),
                        },
                    });
                    debug_assert!(ours_survives || t.is_some());
                    Fate::Keep
                } else {
                    Fate::Delete
                }
            }
        };
    }
    let present = |k: usize| fate[k] == Fate::Keep;

    // 2. Parent of every surviving triple.
    let mut parent: Vec<Option<usize>> = vec![None; n];
    for k in (0..n).filter(|&k| present(k)) {
        let (pb, po, pt) = (cx.parent(k, B), cx.parent(k, O), cx.parent(k, T));
        let chosen = match (po, pt) {
            (Some(a), Some(b)) => {
                if a == b || pb == Some(b) {
                    a
                } else if pb == Some(a) {
                    b
                } else {
                    out.notes.push(Note {
                        kind: NoteKind::CompetingMove,
                        breadcrumb: cx.breadcrumb(k),
                        message: "both sides moved this block; ours was kept".to_owned(),
                    });
                    a
                }
            }
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => None,
        };
        // Re-attach to the nearest surviving ancestor.
        let mut p = chosen;
        let mut hops = 0;
        while let Some(q) = p {
            if present(q) {
                break;
            }
            p = [O, T, B]
                .into_iter()
                .find_map(|s| cx.parent(q, s))
                .unwrap_or(None);
            hops += 1;
        }
        if hops > 0 {
            out.notes.push(Note {
                kind: NoteKind::Reattached,
                breadcrumb: cx.breadcrumb(k),
                message: "its parent was deleted; re-attached to the nearest surviving ancestor"
                    .to_owned(),
            });
        }
        parent[k] = p;
    }
    // Cycles (A under B on one side, B under A on the other): cut at the top level.
    for k in (0..n).filter(|&k| present(k)) {
        let mut cur = parent[k];
        let mut steps = 0;
        while let Some(q) = cur {
            if q == k || steps > n {
                parent[k] = None;
                out.notes.push(Note {
                    kind: NoteKind::CycleBroken,
                    breadcrumb: cx.breadcrumb(k),
                    message: "conflicting moves formed a cycle; the block was put at the top level"
                        .to_owned(),
                });
                break;
            }
            cur = parent[q];
            steps += 1;
        }
    }

    // 3. Sibling order, then a preorder walk.
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); n + 1]; // slot n = top level
    for k in (0..n).filter(|&k| present(k)) {
        kids[parent[k].unwrap_or(n)].push(k);
    }
    let mut ordered: Vec<Vec<usize>> = vec![Vec::new(); n + 1];
    for slot in 0..=n {
        if kids[slot].is_empty() {
            continue;
        }
        let p = (slot < n).then_some(slot);
        let cands = &kids[slot];
        let lists: Vec<Vec<usize>> = [B, O, T]
            .into_iter()
            .map(|s| {
                cx.children(p, s)
                    .into_iter()
                    .filter(|x| cands.contains(x))
                    .collect()
            })
            .collect();
        let (list, competing) = order_siblings(&lists[0], &lists[1], &lists[2], cands);
        if competing {
            out.notes.push(Note {
                kind: NoteKind::CompetingReorder,
                breadcrumb: p.map(|x| cx.breadcrumb(x)).unwrap_or_default(),
                message: "both sides reordered these siblings differently; ours was kept"
                    .to_owned(),
            });
        }
        ordered[slot] = list;
    }
    let mut stack: Vec<(usize, usize)> = ordered[n].iter().rev().map(|&k| (k, 1)).collect();
    while let Some((k, depth)) = stack.pop() {
        out.order.push(PlannedBlock { triple: k, depth });
        stack.extend(ordered[k].iter().rev().map(|&c| (c, depth + 1)));
    }
    out
}

fn block_key(b: &MergeBlock) -> Option<String> {
    match &b.key {
        crate::model::BlockKey::Id(id) => Some(id.clone()),
        crate::model::BlockKey::Synthetic(_) => None,
    }
}

/// Sequence of `a` restricted to the items of `b`.
fn restrict(a: &[usize], b: &[usize]) -> Vec<usize> {
    a.iter().copied().filter(|x| b.contains(x)).collect()
}

/// Three-way merge of a sibling list. Starts from the side that reordered (ours when both did, or
/// neither did) and weaves in the items only the other side has, after their left sibling's
/// match; ours-only items come before theirs-only items at the same spot.
fn order_siblings(
    base: &[usize],
    ours: &[usize],
    theirs: &[usize],
    cands: &[usize],
) -> (Vec<usize>, bool) {
    let ours_reordered = restrict(base, ours) != restrict(ours, base);
    let theirs_reordered = restrict(base, theirs) != restrict(theirs, base);
    let competing =
        ours_reordered && theirs_reordered && restrict(ours, theirs) != restrict(theirs, ours);
    let ours_primary = !(theirs_reordered && !ours_reordered);
    let (primary, secondary) = if ours_primary {
        (ours, theirs)
    } else {
        (theirs, ours)
    };
    let mut result: Vec<usize> = primary.to_vec();
    for (k, &item) in secondary.iter().enumerate() {
        if result.contains(&item) {
            continue;
        }
        let left = secondary[..k].iter().rev().find(|x| result.contains(x));
        let mut pos = left
            .and_then(|l| result.iter().position(|x| x == l))
            .map_or(0, |p| p + 1);
        if ours_primary {
            while pos < result.len() && !secondary.contains(&result[pos]) {
                pos += 1;
            }
        }
        result.insert(pos, item);
    }
    // Items neither sibling list knows (re-attached orphans, moved-in blocks): at the end.
    for &c in cands {
        if !result.contains(&c) {
            result.push(c);
        }
    }
    (result, competing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weave_ours_first_on_same_spot_insert() {
        // base [1], ours [1, 10], theirs [1, 20]
        let (r, c) = order_siblings(&[1], &[1, 10], &[1, 20], &[1, 10, 20]);
        assert_eq!(r, [1, 10, 20]);
        assert!(!c);
    }

    #[test]
    fn weave_takes_theirs_reorder_when_ours_untouched() {
        let (r, c) = order_siblings(&[1, 2, 3], &[1, 2, 3, 9], &[3, 1, 2], &[1, 2, 3, 9]);
        assert_eq!(r, [3, 9, 1, 2]);
        assert!(!c);
    }

    #[test]
    fn weave_competing_reorder_keeps_ours() {
        let (r, c) = order_siblings(&[1, 2, 3], &[2, 1, 3], &[3, 2, 1], &[1, 2, 3]);
        assert_eq!(r, [2, 1, 3]);
        assert!(c);
    }

    #[test]
    fn theirs_insert_without_left_goes_first_after_ours_inserts() {
        let (r, _) = order_siblings(&[1], &[5, 1], &[7, 1], &[1, 5, 7]);
        assert_eq!(r, [5, 7, 1]);
    }
}
