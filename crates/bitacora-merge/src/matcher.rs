//! Block identity matching across base, ours and theirs (`docs/design/git-sync-merge.md` §4.2).
//!
//! Passes, per pair of pages: equal `id::` (global, so moves are tracked); then per matched parent
//! pair: LCS of the children's content hashes, remaining exact content (reorders), fuzzy
//! similarity (best-first, one-to-one); finally a global fuzzy pass for blocks that moved to
//! another parent. Base is matched against ours and theirs, then ours against theirs for blocks
//! added on both sides.
//!
//! Short blocks (fewer than [`MIN_FUZZY_TOKENS`] tokens, e.g. `- TODO`) never match fuzzily, and
//! never across parents: they only pair by exact content under a matched parent.

use std::collections::{HashMap, VecDeque};

use crate::lcs::lcs_pairs;
use crate::model::{BlockKey, MergePage};

/// Similarity threshold of the fuzzy pass.
pub const FUZZY_THRESHOLD: f64 = 0.6;
/// Blocks with fewer first-line tokens than this need an exact match.
pub const MIN_FUZZY_TOKENS: usize = 4;

/// One logical block seen on up to three sides (indices into each page's block arena).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Triple {
    /// Block in base.
    pub base: Option<usize>,
    /// Block in ours.
    pub ours: Option<usize>,
    /// Block in theirs.
    pub theirs: Option<usize>,
}

/// The result of [`match_blocks`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Matching {
    /// Base blocks in base order, then blocks only on the other sides (ours first, in document
    /// order, then theirs-only).
    pub triples: Vec<Triple>,
}

/// Matches the blocks of the three pages.
#[must_use]
pub fn match_blocks(base: &MergePage, ours: &MergePage, theirs: &MergePage) -> Matching {
    let b_to_o = match_pages(base, ours);
    let b_to_t = match_pages(base, theirs);

    let mut triples: Vec<Triple> = (0..base.blocks.len())
        .map(|i| Triple {
            base: Some(i),
            ours: b_to_o[i],
            theirs: b_to_t[i],
        })
        .collect();

    let mut o_used = vec![false; ours.blocks.len()];
    let mut t_used = vec![false; theirs.blocks.len()];
    for t in &triples {
        if let Some(o) = t.ours {
            o_used[o] = true;
        }
        if let Some(x) = t.theirs {
            t_used[x] = true;
        }
    }

    // Ours against theirs for blocks that have no base: seeded with the pairs already known.
    let seeds: Vec<(usize, usize)> = triples
        .iter()
        .filter_map(|t| t.ours.zip(t.theirs))
        .collect();
    let allowed_o: Vec<bool> = o_used.iter().map(|u| !u).collect();
    let allowed_t: Vec<bool> = t_used.iter().map(|u| !u).collect();
    let mut p = Pairing::new(ours, theirs, allowed_o, allowed_t);
    p.run(&seeds);
    let o_to_t = p.ab;

    let mut t_paired = vec![false; theirs.blocks.len()];
    for (o, partner) in o_to_t.iter().enumerate() {
        if o_used[o] {
            continue;
        }
        if let Some(x) = *partner {
            t_paired[x] = true;
        }
        triples.push(Triple {
            base: None,
            ours: Some(o),
            theirs: *partner,
        });
    }
    for x in 0..theirs.blocks.len() {
        if !t_used[x] && !t_paired[x] {
            triples.push(Triple {
                base: None,
                ours: None,
                theirs: Some(x),
            });
        }
    }
    salvage_short_edits(&mut triples, [base, ours, theirs]);
    Matching { triples }
}

/// Last pass: a short block edited on one side (`- Draft` -> `- Draft v2`) is too short for the
/// fuzzy pass, so the other side sees it as "deleted" next to an unrelated insert. Re-pair the two
/// when, under the same parent, the new first line extends (or is a prefix of) the old one and
/// the pairing is unambiguous.
fn salvage_short_edits(triples: &mut Vec<Triple>, pages: [&MergePage; 3]) {
    let get = |t: &Triple, side: usize| [t.base, t.ours, t.theirs][side];
    for (keep, new) in [(1usize, 2usize), (2, 1)] {
        // base block index -> triple index
        let mut by_base = vec![usize::MAX; pages[0].blocks.len()];
        for (k, t) in triples.iter().enumerate() {
            if let Some(b) = t.base {
                by_base[b] = k;
            }
        }
        let olds: Vec<usize> = (0..triples.len())
            .filter(|&k| {
                let t = &triples[k];
                t.base.is_some() && get(t, new).is_none()
            })
            .collect();
        let news: Vec<usize> = (0..triples.len())
            .filter(|&k| {
                let t = &triples[k];
                t.base.is_none() && get(t, keep).is_none() && get(t, new).is_some()
            })
            .collect();
        let related = |x: usize, y: usize| -> bool {
            let (Some(b), Some(n)) = (triples[x].base, get(&triples[y], new)) else {
                return false;
            };
            let (bb, nb) = (&pages[0].blocks[b], &pages[new].blocks[n]);
            let parent_ok = match (bb.parent, nb.parent) {
                (None, None) => true,
                (Some(pb), Some(pn)) => get(&triples[by_base[pb]], new) == Some(pn),
                _ => false,
            };
            let (a, c) = (bb.first_line().trim(), nb.first_line().trim());
            let (short, long) = if a.len() <= c.len() { (a, c) } else { (c, a) };
            parent_ok && !short.is_empty() && long.starts_with(short)
        };
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for &x in &olds {
            let ys: Vec<usize> = news.iter().copied().filter(|&y| related(x, y)).collect();
            if let [y] = ys[..]
                && olds.iter().filter(|&&x2| related(x2, y)).count() == 1
            {
                pairs.push((x, y));
            }
        }
        let mut removed = Vec::new();
        for (x, y) in pairs {
            let v = get(&triples[y], new);
            if new == 1 {
                triples[x].ours = v;
            } else {
                triples[x].theirs = v;
            }
            removed.push(y);
        }
        removed.sort_unstable();
        for y in removed.into_iter().rev() {
            triples.remove(y);
        }
    }
}

/// Matches every block of `a` to at most one block of `b`; returns `a index -> b index`.
#[must_use]
pub fn match_pages(a: &MergePage, b: &MergePage) -> Vec<Option<usize>> {
    let mut p = Pairing::new(a, b, vec![true; a.blocks.len()], vec![true; b.blocks.len()]);
    p.run(&[]);
    p.ab
}

struct Pairing<'a> {
    a: &'a MergePage,
    b: &'a MergePage,
    allowed_a: Vec<bool>,
    allowed_b: Vec<bool>,
    ab: Vec<Option<usize>>,
    ba: Vec<Option<usize>>,
    queue: VecDeque<(Option<usize>, Option<usize>)>,
    hash_a: Vec<u64>,
    hash_b: Vec<u64>,
}

impl<'a> Pairing<'a> {
    fn new(a: &'a MergePage, b: &'a MergePage, allowed_a: Vec<bool>, allowed_b: Vec<bool>) -> Self {
        Self {
            a,
            b,
            allowed_a,
            allowed_b,
            ab: vec![None; a.blocks.len()],
            ba: vec![None; b.blocks.len()],
            queue: VecDeque::new(),
            hash_a: a.blocks.iter().map(|x| x.normalized_hash()).collect(),
            hash_b: b.blocks.iter().map(|x| x.normalized_hash()).collect(),
        }
    }

    fn free_a(&self, i: usize) -> bool {
        self.allowed_a[i] && self.ab[i].is_none()
    }

    fn free_b(&self, j: usize) -> bool {
        self.allowed_b[j] && self.ba[j].is_none()
    }

    fn link(&mut self, i: usize, j: usize) {
        self.ab[i] = Some(j);
        self.ba[j] = Some(i);
        self.queue.push_back((Some(i), Some(j)));
    }

    fn run(&mut self, seeds: &[(usize, usize)]) {
        // Pairs already known (not matchable themselves) still anchor their children.
        for &(i, j) in seeds {
            self.queue.push_back((Some(i), Some(j)));
        }
        self.pass_ids();
        self.queue.push_back((None, None));
        self.drain();
        loop {
            let before = self.queue.len();
            self.pass_global_fuzzy();
            if self.queue.len() == before {
                break;
            }
            self.drain();
        }
    }

    /// Pass 1: equal `id::` anywhere in the page.
    fn pass_ids(&mut self) {
        let mut by_id: HashMap<&str, usize> = HashMap::new();
        for blk in &self.b.blocks {
            if let BlockKey::Id(id) = &blk.key {
                by_id.entry(id.as_str()).or_insert(blk.index);
            }
        }
        let mut found = Vec::new();
        for blk in &self.a.blocks {
            if let BlockKey::Id(id) = &blk.key
                && let Some(&j) = by_id.get(id.as_str())
                && self.free_a(blk.index)
                && self.free_b(j)
            {
                found.push((blk.index, j));
            }
        }
        for (i, j) in found {
            self.link(i, j);
        }
    }

    fn drain(&mut self) {
        while let Some((pa, pb)) = self.queue.pop_front() {
            self.match_children(pa, pb);
        }
    }

    fn match_children(&mut self, pa: Option<usize>, pb: Option<usize>) {
        let ca: Vec<usize> = self
            .a
            .children_of(pa)
            .iter()
            .copied()
            .filter(|&i| self.free_a(i))
            .collect();
        let cb: Vec<usize> = self
            .b
            .children_of(pb)
            .iter()
            .copied()
            .filter(|&j| self.free_b(j))
            .collect();
        if ca.is_empty() || cb.is_empty() {
            return;
        }

        // Pass 3: LCS of content hashes (keeps order, disambiguates duplicates like "- TODO").
        let ha: Vec<u64> = ca.iter().map(|&i| self.hash_a[i]).collect();
        let hb: Vec<u64> = cb.iter().map(|&j| self.hash_b[j]).collect();
        for (x, y) in lcs_pairs(&ha, &hb) {
            self.link(ca[x], cb[y]);
        }
        // Pass 2: remaining exact content under the same parent (reordered siblings).
        for &i in &ca {
            if !self.free_a(i) {
                continue;
            }
            if let Some(&j) = cb
                .iter()
                .find(|&&j| self.free_b(j) && self.hash_b[j] == self.hash_a[i])
            {
                self.link(i, j);
            }
        }
        // Pass 4: fuzzy among what is left under this parent pair.
        let ra: Vec<usize> = ca.iter().copied().filter(|&i| self.free_a(i)).collect();
        let rb: Vec<usize> = cb.iter().copied().filter(|&j| self.free_b(j)).collect();
        self.fuzzy(&ra, &rb);
    }

    /// Greedy best-first one-to-one fuzzy matching between two candidate lists.
    fn fuzzy(&mut self, ra: &[usize], rb: &[usize]) {
        if ra.is_empty() || rb.is_empty() {
            return;
        }
        let mut cands: Vec<(u32, usize, usize, usize)> = Vec::new();
        for (x, &i) in ra.iter().enumerate() {
            for (y, &j) in rb.iter().enumerate() {
                if let Some(s) =
                    similarity(self.a.blocks[i].first_line(), self.b.blocks[j].first_line())
                {
                    // Sort key: higher score first, then closer position, then document order.
                    cands.push((s, x.abs_diff(y), i, j));
                }
            }
        }
        cands.sort_by(|p, q| {
            q.0.cmp(&p.0)
                .then(p.1.cmp(&q.1))
                .then(p.2.cmp(&q.2))
                .then(p.3.cmp(&q.3))
        });
        for (_, _, i, j) in cands {
            if self.free_a(i) && self.free_b(j) {
                self.link(i, j);
            }
        }
    }

    /// Fuzzy pass over every remaining block regardless of parent (moved blocks). Only blocks with
    /// at least [`MIN_FUZZY_TOKENS`] tokens take part.
    fn pass_global_fuzzy(&mut self) {
        let long = |s: &str| s.split_whitespace().count() >= MIN_FUZZY_TOKENS;
        let ra: Vec<usize> = (0..self.a.blocks.len())
            .filter(|&i| self.free_a(i) && long(self.a.blocks[i].first_line()))
            .collect();
        let rb: Vec<usize> = (0..self.b.blocks.len())
            .filter(|&j| self.free_b(j) && long(self.b.blocks[j].first_line()))
            .collect();
        self.fuzzy(&ra, &rb);
    }
}

/// Similarity of two first lines in thousandths, or `None` below the threshold, when either line
/// is too short for a fuzzy match, or when it is not a match at all.
#[must_use]
pub fn similarity(a: &str, b: &str) -> Option<u32> {
    let ta: Vec<String> = a.split_whitespace().map(str::to_lowercase).collect();
    let tb: Vec<String> = b.split_whitespace().map(str::to_lowercase).collect();
    if ta.len() < MIN_FUZZY_TOKENS || tb.len() < MIN_FUZZY_TOKENS {
        return None;
    }
    let s = jaccard(&ta, &tb).max(levenshtein_similarity(a, b));
    (s >= FUZZY_THRESHOLD).then(|| (s * 1000.0).round() as u32)
}

fn jaccard(a: &[String], b: &[String]) -> f64 {
    let sa: std::collections::BTreeSet<&String> = a.iter().collect();
    let sb: std::collections::BTreeSet<&String> = b.iter().collect();
    let inter = sa.intersection(&sb).count();
    let union = sa.union(&sb).count();
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

/// `1 - levenshtein / max_len` on the first 256 chars.
fn levenshtein_similarity(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.to_lowercase().chars().take(256).collect();
    let b: Vec<char> = b.to_lowercase().chars().take(256).collect();
    let max = a.len().max(b.len());
    if max == 0 {
        return 1.0;
    }
    // Length difference alone bounds the distance: skip hopeless pairs.
    if (a.len().min(b.len()) as f64) / (max as f64) < FUZZY_THRESHOLD {
        return 0.0;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    1.0 - prev[b.len()] as f64 / max as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages(b: &str, o: &str, t: &str) -> (MergePage, MergePage, MergePage) {
        (
            MergePage::parse(b),
            MergePage::parse(o),
            MergePage::parse(t),
        )
    }

    fn triple_of(m: &Matching, base: usize) -> Triple {
        *m.triples
            .iter()
            .find(|t| t.base == Some(base))
            .expect("triple")
    }

    const ID1: &str = "66aa0000-0000-4000-8000-000000000001";
    const ID2: &str = "66bb0000-0000-4000-8000-000000000002";

    #[test]
    fn pass1_match_by_id_even_when_content_and_parent_change() {
        let (b, o, t) = pages(
            &format!("- Plan\n  id:: {ID1}\n- Other\n"),
            &format!("- Plan\n  id:: {ID1}\n- Other\n"),
            &format!("- Other\n  - Plan for Q4 totally rewritten\n    id:: {ID1}\n"),
        );
        let m = match_blocks(&b, &o, &t);
        let tr = triple_of(&m, 0);
        assert_eq!(tr.ours, Some(0));
        assert_eq!(tr.theirs, Some(1));
    }

    #[test]
    fn pass2_exact_content_under_same_parent_and_pure_reorder() {
        let (b, o, t) = pages("- A\n- B\n- C\n", "- C\n- A\n- B\n", "- A\n- B\n- C\n");
        let m = match_blocks(&b, &o, &t);
        assert_eq!(triple_of(&m, 0).ours, Some(1));
        assert_eq!(triple_of(&m, 1).ours, Some(2));
        assert_eq!(triple_of(&m, 2).ours, Some(0));
        assert!(m.triples.iter().all(|t| t.base.is_some()));
    }

    #[test]
    fn pass3_lcs_pairs_duplicates_in_order() {
        let (b, o, t) = pages(
            "- TODO\n- x\n- TODO\n",
            "- TODO\n- x\n- TODO\n- new thing here now\n",
            "- TODO\n- x\n- TODO\n",
        );
        let m = match_blocks(&b, &o, &t);
        assert_eq!(triple_of(&m, 0).ours, Some(0));
        assert_eq!(triple_of(&m, 2).ours, Some(2));
    }

    #[test]
    fn pass4_fuzzy_matches_edited_block() {
        let (b, o, t) = pages(
            "- Buy milk and eggs\n",
            "- Buy milk and eggs today\n",
            "- Buy milk and eggs\n",
        );
        let m = match_blocks(&b, &o, &t);
        let tr = triple_of(&m, 0);
        assert_eq!((tr.ours, tr.theirs), (Some(0), Some(0)));
    }

    #[test]
    fn added_id_still_matches_and_is_metadata() {
        let (b, o, t) = pages(
            "- Idea A\n",
            &format!("- Idea A\n  id:: {ID2}\n"),
            "- Idea A\n",
        );
        let m = match_blocks(&b, &o, &t);
        let tr = triple_of(&m, 0);
        assert_eq!(tr.ours, Some(0));
        assert!(b.blocks[0].normalized_hash() == o.blocks[0].normalized_hash());
    }

    #[test]
    fn short_blocks_are_not_fuzzy_matched_but_unambiguous_extensions_are_salvaged() {
        // Not a prefix relation: still distinct (delete + insert).
        let (b, o, t) = pages("- TODO\n", "- DONE x\n", "- TODO\n");
        let m = match_blocks(&b, &o, &t);
        assert_eq!(triple_of(&m, 0).ours, None);
        // "- TODO" -> "- TODO x" under the same parent, one candidate: re-paired.
        let (b, o, t) = pages("- TODO\n", "- TODO x\n", "- TODO\n");
        let m = match_blocks(&b, &o, &t);
        assert_eq!(triple_of(&m, 0).ours, Some(0));
        assert_eq!(m.triples.len(), 1);
    }

    #[test]
    fn short_blocks_do_not_mispair_across_parents() {
        let (b, o, t) = pages(
            "- A\n  - TODO\n- B\n",
            "- A\n- B\n  - TODO\n",
            "- A\n  - TODO\n- B\n",
        );
        let m = match_blocks(&b, &o, &t);
        assert_eq!(triple_of(&m, 1).ours, None);
    }

    #[test]
    fn moved_long_block_is_tracked_across_parents() {
        let (b, o, t) = pages(
            "- A\n  - write the quarterly report now\n- B\n",
            "- A\n- B\n  - write the quarterly report now\n",
            "- A\n  - write the quarterly report now\n- B\n",
        );
        let m = match_blocks(&b, &o, &t);
        assert_eq!(triple_of(&m, 1).ours, Some(2));
        assert_eq!(triple_of(&m, 1).theirs, Some(1));
    }

    #[test]
    fn both_added_identical_block_is_paired_ours_theirs() {
        let (b, o, t) = pages(
            "- A\n",
            "- A\n- new block here\n",
            "- A\n- new block here\n",
        );
        let m = match_blocks(&b, &o, &t);
        let added = m.triples.iter().find(|t| t.base.is_none()).expect("added");
        assert_eq!((added.ours, added.theirs), (Some(1), Some(1)));
        assert_eq!(m.triples.len(), 2);
    }

    #[test]
    fn different_inserts_stay_distinct() {
        let (b, o, t) = pages(
            "- A\n- C\n",
            "- A\n- B1 something long enough\n- C\n",
            "- A\n- B2 another thing entirely\n- C\n",
        );
        let m = match_blocks(&b, &o, &t);
        let added: Vec<_> = m.triples.iter().filter(|t| t.base.is_none()).collect();
        assert_eq!(added.len(), 2);
        assert!(added.iter().all(|t| t.ours.is_none() || t.theirs.is_none()));
    }

    #[test]
    fn match_is_one_to_one() {
        let (b, o, t) = pages(
            "- the quick brown fox jumps\n- the quick brown fox jumped\n",
            "- the quick brown fox jumps over\n",
            "- the quick brown fox jumps\n- the quick brown fox jumped\n",
        );
        let m = match_blocks(&b, &o, &t);
        let mut seen = std::collections::HashSet::new();
        for tr in &m.triples {
            if let Some(o) = tr.ours {
                assert!(seen.insert(o), "ours block used twice");
            }
        }
    }

    #[test]
    fn crlf_and_tab_indentation_do_not_break_matching() {
        let (b, o, t) = pages(
            "- Parent\n  - child with enough words here\n",
            "- Parent\r\n\t- child with enough words here\r\n",
            "- Parent\n  - child with enough words here\n",
        );
        let m = match_blocks(&b, &o, &t);
        assert_eq!(triple_of(&m, 1).ours, Some(1));
    }

    #[test]
    fn similarity_scores() {
        assert!(similarity("Buy milk and eggs", "Buy milk and eggs today").is_some());
        assert!(similarity("Buy milk and eggs", "Totally different words appear here").is_none());
        assert!(similarity("TODO", "TODO").is_none());
    }
}
