//! Tolerant relative-indent tree builder.
//!
//! Logseq does not require consistent indentation: the parent/child structure comes from comparing
//! the raw levels (indent characters + 1) of consecutive blocks. The behaviour implemented here is
//! the documented one (`docs/analysis/logseq/02-markdown-block-syntax.md` §2.1), written from
//! scratch (ADR-015):
//!
//! * a block with a greater level than the previous *open* block becomes its child, whatever the
//!   difference;
//! * an equal level makes it a sibling;
//! * on an outdent it pops to the nearest open ancestor with exactly that level and becomes its
//!   sibling; when no ancestor has exactly that level, it becomes a sibling of the first open block
//!   that is deeper than it, and **adopts that block's level** for the rest of the page.

use crate::outline::RawBlock;

/// Where a block sits in the tree. Indices refer to the block list passed to [`build_tree`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeLinks {
    /// Parent block, `None` for top-level blocks (children of the page).
    pub parent: Option<usize>,
    /// The sibling right before this block under the same parent.
    pub prev_sibling: Option<usize>,
    /// 1-based tree depth (top-level blocks have depth 1).
    pub depth: usize,
}

struct Open {
    /// Block index, `None` for the virtual page root.
    idx: Option<usize>,
    /// Effective level used for comparisons (can differ from the raw level after a tolerant outdent).
    level: usize,
    depth: usize,
}

/// Builds parent/previous-sibling links for `blocks`, in document order.
///
/// Parents always precede their children, so the result is a forest by construction.
#[must_use]
pub fn build_tree(blocks: &[RawBlock]) -> Vec<NodeLinks> {
    build_tree_from_levels(&blocks.iter().map(|b| b.raw_level).collect::<Vec<_>>())
}

/// Same as [`build_tree`], from raw levels only.
#[must_use]
pub fn build_tree_from_levels(levels: &[usize]) -> Vec<NodeLinks> {
    let mut out = Vec::with_capacity(levels.len());
    let mut stack = vec![Open {
        idx: None,
        level: 0,
        depth: 0,
    }];
    for (i, &raw) in levels.iter().enumerate() {
        let mut level = raw;
        loop {
            let top_level = stack.last().map_or(0, |t| t.level);
            if level == top_level && stack.len() > 1 {
                // Sibling: replaces the top of the stack.
                let top = stack.pop().unwrap_or(Open {
                    idx: None,
                    level: 0,
                    depth: 1,
                });
                let parent = stack.last().and_then(|p| p.idx);
                out.push(NodeLinks {
                    parent,
                    prev_sibling: top.idx,
                    depth: top.depth,
                });
                stack.push(Open {
                    idx: Some(i),
                    level,
                    depth: top.depth,
                });
                break;
            }
            if level > top_level {
                // Child of the top (or a top-level block when the top is the page root).
                let (parent, depth) = stack.last().map_or((None, 1), |t| (t.idx, t.depth + 1));
                out.push(NodeLinks {
                    parent,
                    prev_sibling: None,
                    depth,
                });
                stack.push(Open {
                    idx: Some(i),
                    level,
                    depth,
                });
                break;
            }
            // Outdent: level < top_level.
            if stack.iter().any(|o| o.level == level) {
                stack.truncate(stack.iter().take_while(|o| o.level <= level).count());
                continue;
            }
            let keep = stack.iter().take_while(|o| o.level <= level).count();
            let tail = stack.split_off(keep);
            let adopted = tail.first();
            let (adopted_level, adopted_idx, depth) =
                adopted.map_or((level, None, 1), |a| (a.level, a.idx, a.depth));
            let parent = stack.last().and_then(|p| p.idx);
            out.push(NodeLinks {
                parent,
                prev_sibling: adopted_idx,
                depth,
            });
            level = adopted_level;
            stack.push(Open {
                idx: Some(i),
                level,
                depth,
            });
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::split;

    fn tree(input: &str) -> Vec<(Option<usize>, Option<usize>, usize)> {
        let o = split(input.as_bytes());
        build_tree(&o.blocks)
            .into_iter()
            .map(|n| (n.parent, n.prev_sibling, n.depth))
            .collect()
    }

    #[test]
    fn regular_nesting() {
        assert_eq!(
            tree("- a\n  - b\n    - c\n  - d\n- e"),
            [
                (None, None, 1),
                (Some(0), None, 2),
                (Some(1), None, 3),
                (Some(0), Some(1), 2),
                (None, Some(0), 1),
            ]
        );
    }

    #[test]
    fn greater_level_is_a_child_whatever_the_delta() {
        assert_eq!(
            tree("- a\n            - b"),
            [(None, None, 1), (Some(0), None, 2)]
        );
    }

    #[test]
    fn irregular_indent_outdent_without_exact_match() {
        // Levels 1, 5, 7, 6: the last block has no ancestor at level 6, so it becomes a sibling of
        // the first deeper block (level 7), i.e. a child of `line2`.
        assert_eq!(
            tree("- line1\n    - line2\n      - line3\n     - line4"),
            [
                (None, None, 1),
                (Some(0), None, 2),
                (Some(1), None, 3),
                (Some(1), Some(2), 3),
            ]
        );
    }

    #[test]
    fn adopted_level_applies_to_following_blocks() {
        // After the tolerant outdent, `d` carries level 7; the next level-7 block is its sibling.
        let t = tree("- a\n    - b\n      - c\n     - d\n      - e");
        assert_eq!(t[3], (Some(1), Some(2), 3));
        assert_eq!(t[4], (Some(1), Some(3), 3));
    }

    #[test]
    fn heading_with_indented_bullets() {
        assert_eq!(
            tree("## hello\n    - world"),
            [(None, None, 1), (Some(0), None, 2)]
        );
    }

    #[test]
    fn deep_start_then_shallower_blocks() {
        // The first block is indented: it is still top level; a later shallower block is a sibling
        // of the first deeper open block and adopts its level (5), so a level-3 block that follows
        // is again a sibling rather than a child.
        assert_eq!(
            tree("    - a\n- b\n  - c"),
            [(None, None, 1), (None, Some(0), 1), (None, Some(1), 1)]
        );
    }

    #[test]
    fn mixed_tabs_and_spaces_count_one_each() {
        // Levels 1, 2 (tab), 3 (two spaces), 2 (tab): `c` is deeper than `b`, the last tab block
        // is a sibling of `b`.
        assert_eq!(
            tree("- a\n\t- b\n  - c\n\t- d"),
            [
                (None, None, 1),
                (Some(0), None, 2),
                (Some(1), None, 3),
                (Some(0), Some(1), 2),
            ]
        );
    }

    #[test]
    fn empty_input() {
        assert!(build_tree_from_levels(&[]).is_empty());
    }

    #[test]
    fn tree_is_always_a_forest_with_parents_first() {
        // Exhaustive over all level sequences up to length 6 with levels in 1..=4.
        fn walk(seq: &mut Vec<usize>, max_len: usize) {
            if !seq.is_empty() {
                let links = build_tree_from_levels(seq);
                assert_eq!(links.len(), seq.len());
                for (i, n) in links.iter().enumerate() {
                    if let Some(p) = n.parent {
                        assert!(p < i);
                        assert_eq!(n.depth, links[p].depth + 1);
                    } else {
                        assert_eq!(n.depth, 1);
                    }
                    if let Some(s) = n.prev_sibling {
                        assert!(s < i);
                        assert_eq!(links[s].parent, n.parent);
                        assert_eq!(links[s].depth, n.depth);
                    }
                }
            }
            if seq.len() == max_len {
                return;
            }
            for l in 1..=4 {
                seq.push(l);
                walk(seq, max_len);
                seq.pop();
            }
        }
        walk(&mut Vec::new(), 6);
    }
}
