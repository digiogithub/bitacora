//! Content diff3 within a block and per-key user property merge (BIT-SP-0006.R10).

use crate::conflict::{Conflict, ConflictKind};
use crate::lcs::lcs_pairs;
use crate::marker::merge_title;
use crate::model::PropEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Hunk {
    start: usize,
    end: usize,
    repl: Vec<String>,
}

/// The edits that turn `base` into `side`, as replacements of base ranges.
fn hunks(base: &[&str], side: &[&str]) -> Vec<Hunk> {
    let pairs = lcs_pairs(base, side);
    let mut out = Vec::new();
    let (mut bi, mut si) = (0, 0);
    let mut flush = |b_end: usize, s_end: usize, bi: usize, si: usize| {
        if b_end > bi || s_end > si {
            out.push(Hunk {
                start: bi,
                end: b_end,
                repl: side[si..s_end].iter().map(|s| (*s).to_owned()).collect(),
            });
        }
    };
    for (i, j) in pairs {
        flush(i, j, bi, si);
        bi = i + 1;
        si = j + 1;
    }
    flush(base.len(), side.len(), bi, si);
    out
}

/// Whether a hunk collides with the base region `[s, e)` (insertions are empty ranges).
fn collides(h: &Hunk, s: usize, e: usize) -> bool {
    match (h.start == h.end, s == e) {
        (false, false) => h.start < e && s < h.end,
        (true, true) => h.start == s,
        (true, false) => s <= h.start && h.start < e,
        (false, true) => h.start <= s && s < h.end,
    }
}

fn apply(base: &[&str], s: usize, e: usize, hunks: &[&Hunk]) -> Vec<String> {
    let mut out = Vec::new();
    let mut pos = s;
    for h in hunks {
        out.extend(base[pos..h.start].iter().map(|l| (*l).to_owned()));
        out.extend(h.repl.iter().cloned());
        pos = h.end;
    }
    out.extend(base[pos..e].iter().map(|l| (*l).to_owned()));
    out
}

/// Result of a line-level three-way merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff3 {
    /// Merged lines; every conflicting region holds ours.
    pub lines: Vec<String>,
    /// Number of conflicting regions.
    pub conflicts: usize,
}

/// Classic diff3: non-overlapping edits of both sides combine; overlapping, different edits of the
/// same base lines are conflicts (ours kept).
#[must_use]
pub fn diff3(base: &[&str], ours: &[&str], theirs: &[&str]) -> Diff3 {
    let mut remaining: Vec<(bool, Hunk)> = hunks(base, ours)
        .into_iter()
        .map(|h| (true, h))
        .chain(hunks(base, theirs).into_iter().map(|h| (false, h)))
        .collect();
    remaining.sort_by_key(|a| (a.1.start, a.1.end, !a.0));

    let mut lines: Vec<String> = Vec::new();
    let mut conflicts = 0;
    let mut pos = 0;
    while !remaining.is_empty() {
        let seed = remaining.remove(0);
        let (mut s, mut e) = (seed.1.start, seed.1.end);
        let mut group = vec![seed];
        while let Some(idx) = remaining.iter().position(|(_, h)| collides(h, s, e)) {
            let h = remaining.remove(idx);
            s = s.min(h.1.start);
            e = e.max(h.1.end);
            group.push(h);
        }
        group.sort_by_key(|a| (a.1.start, a.1.end));
        lines.extend(base[pos..s].iter().map(|l| (*l).to_owned()));
        let of: Vec<&Hunk> = group.iter().filter(|g| g.0).map(|g| &g.1).collect();
        let tf: Vec<&Hunk> = group.iter().filter(|g| !g.0).map(|g| &g.1).collect();
        if tf.is_empty() {
            lines.extend(apply(base, s, e, &of));
        } else if of.is_empty() {
            lines.extend(apply(base, s, e, &tf));
        } else {
            let (o, t) = (apply(base, s, e, &of), apply(base, s, e, &tf));
            if o != t {
                conflicts += 1;
            }
            lines.extend(o);
        }
        pos = e;
    }
    lines.extend(base[pos..].iter().map(|l| (*l).to_owned()));
    Diff3 { lines, conflicts }
}

/// Result of merging one field: the merged value (ours when conflicting) and the conflict, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldResult<T> {
    /// Merged value; holds ours when `conflict` is set.
    pub value: T,
    /// The unresolved conflict, if any.
    pub conflict: Option<Conflict>,
}

/// Three-way merge of a block's content (first line plus continuation lines).
///
/// Short-circuits equalities, merges the first line with the task marker rules
/// ([`merge_title`]) and the remaining lines with [`diff3`]. Any overlap is a
/// [`ConflictKind::Content`] conflict whose output is ours. `base` is `None` when both sides added
/// the block.
#[must_use]
pub fn merge_content(base: Option<&str>, ours: &str, theirs: &str) -> FieldResult<String> {
    let clean = |value: String| FieldResult {
        value,
        conflict: None,
    };
    if ours == theirs {
        return clean(ours.to_owned());
    }
    if let Some(b) = base {
        if ours == b {
            return clean(theirs.to_owned());
        }
        if theirs == b {
            return clean(ours.to_owned());
        }
    }
    let split = |s: &'_ str| -> (String, Vec<String>) {
        let mut it = s.lines();
        let first = it.next().unwrap_or("").to_owned();
        (first, it.map(str::to_owned).collect())
    };
    let (bt, br) = split(base.unwrap_or(""));
    let (ot, or) = split(ours);
    let (tt, tr) = split(theirs);
    let title = merge_title(&bt, &ot, &tt);
    fn refs(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }
    let rest = diff3(&refs(&br), &refs(&or), &refs(&tr));
    match title {
        Some(title) if rest.conflicts == 0 => {
            let mut out = title;
            for l in rest.lines {
                out.push('\n');
                out.push_str(&l);
            }
            clean(out)
        }
        _ => FieldResult {
            value: ours.to_owned(),
            conflict: Some(Conflict {
                kind: ConflictKind::Content,
                field: "content".to_owned(),
                base: base.map(str::to_owned),
                ours: Some(ours.to_owned()),
                theirs: Some(theirs.to_owned()),
            }),
        },
    }
}

/// Per-key three-way merge of user (content-class) properties. The same key changed differently on
/// both sides, or removed on one side and changed on the other, is a
/// [`ConflictKind::Property`] conflict (ours kept). Output order: ours, then keys only theirs has,
/// in theirs' relative order.
#[must_use]
pub fn merge_user_props(
    base: &[PropEntry],
    ours: &[PropEntry],
    theirs: &[PropEntry],
) -> (Vec<PropEntry>, Vec<Conflict>) {
    let get = |v: &'_ [PropEntry], k: &str| v.iter().find(|p| p.norm == k).cloned();
    let mut keys: Vec<&str> = ours.iter().map(|p| p.norm.as_str()).collect();
    for p in theirs {
        if !keys.contains(&p.norm.as_str()) {
            keys.push(&p.norm);
        }
    }
    let val = |p: &Option<PropEntry>| p.as_ref().map(|p| p.value.clone());
    let mut out = Vec::new();
    let mut conflicts = Vec::new();
    for k in keys {
        let (b, o, t) = (get(base, k), get(ours, k), get(theirs, k));
        let (vb, vo, vt) = (val(&b), val(&o), val(&t));
        let chosen = if vo == vt || vt == vb {
            o
        } else if vo == vb {
            t
        } else {
            conflicts.push(Conflict {
                kind: ConflictKind::Property,
                field: k.to_owned(),
                base: vb,
                ours: vo,
                theirs: vt,
            });
            o
        };
        out.extend(chosen);
    }
    (out, conflicts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(k: &str, v: &str) -> PropEntry {
        PropEntry {
            key: k.to_owned(),
            norm: k.to_owned(),
            value: v.to_owned(),
        }
    }

    fn d3(b: &[&str], o: &[&str], t: &[&str]) -> Diff3 {
        diff3(b, o, t)
    }

    #[test]
    fn diff3_non_overlapping_edits_merge() {
        let r = d3(&["a", "b", "c"], &["A", "b", "c"], &["a", "b", "C"]);
        assert_eq!(r.lines, ["A", "b", "C"]);
        assert_eq!(r.conflicts, 0);
        // Adjacent lines are different lines.
        let r = d3(&["a", "b"], &["A", "b"], &["a", "B"]);
        assert_eq!(r.lines, ["A", "B"]);
        assert_eq!(r.conflicts, 0);
    }

    #[test]
    fn diff3_both_append_different_lines_conflicts_same_appends_merge() {
        let r = d3(&["a"], &["a", "x"], &["a", "y"]);
        assert_eq!(r.conflicts, 1);
        assert_eq!(r.lines, ["a", "x"]);
        let r = d3(&["a"], &["a", "x"], &["a", "x"]);
        assert_eq!(
            (r.conflicts, r.lines),
            (0, vec!["a".to_owned(), "x".to_owned()])
        );
    }

    #[test]
    fn diff3_overlap_conflict_keeps_ours_and_continues() {
        let r = d3(
            &["a", "b", "c", "d"],
            &["a", "B1", "c", "D"],
            &["a", "B2", "c", "d"],
        );
        assert_eq!(r.conflicts, 1);
        assert_eq!(r.lines, ["a", "B1", "c", "D"]);
    }

    #[test]
    fn diff3_delete_and_insert_elsewhere() {
        let r = d3(&["a", "b", "c"], &["a", "c"], &["a", "b", "c", "d"]);
        assert_eq!(r.lines, ["a", "c", "d"]);
        assert_eq!(r.conflicts, 0);
    }

    #[test]
    fn r10_different_lines_of_a_multiline_block() {
        let r = merge_content(
            Some("Notes\nline a"),
            "Notes\nline a\nline b",
            "Notes (draft)\nline a",
        );
        assert_eq!(r.value, "Notes (draft)\nline a\nline b");
        assert!(r.conflict.is_none());
    }

    #[test]
    fn r10_same_line_edited_both_sides_conflicts_with_ours() {
        let r = merge_content(Some("Meet at 10"), "Meet at 11", "Meet at 12");
        assert_eq!(r.value, "Meet at 11");
        let c = r.conflict.expect("conflict");
        assert_eq!(c.kind, ConflictKind::Content);
        assert_eq!(c.theirs.as_deref(), Some("Meet at 12"));
        assert_eq!(c.base.as_deref(), Some("Meet at 10"));
    }

    #[test]
    fn content_short_circuits() {
        assert_eq!(merge_content(Some("a"), "a", "b").value, "b");
        assert_eq!(merge_content(Some("a"), "b", "a").value, "b");
        assert_eq!(merge_content(Some("a"), "b", "b").value, "b");
        assert!(merge_content(None, "x", "x").conflict.is_none());
        assert!(merge_content(None, "x", "y").conflict.is_some());
    }

    #[test]
    fn r13_marker_inside_content_merge() {
        let r = merge_content(
            Some("TODO write report\nbody"),
            "DONE write report\nbody",
            "TODO write final report\nbody\nmore",
        );
        assert_eq!(r.value, "DONE write final report\nbody\nmore");
        assert!(r.conflict.is_none());
    }

    #[test]
    fn r10_property_per_key() {
        let base = [p("status", "open")];
        let ours = [p("status", "open"), p("owner", "ana")];
        let theirs = [p("status", "done")];
        let (out, c) = merge_user_props(&base, &ours, &theirs);
        assert!(c.is_empty());
        assert_eq!(out, [p("status", "done"), p("owner", "ana")]);
    }

    #[test]
    fn property_same_key_divergent_conflicts_keeps_ours() {
        let base = [p("status", "open")];
        let (out, c) = merge_user_props(&base, &[p("status", "wip")], &[p("status", "done")]);
        assert_eq!(out, [p("status", "wip")]);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].kind, ConflictKind::Property);
        assert_eq!(c[0].field, "status");
    }

    #[test]
    fn property_add_remove_rules_and_order() {
        // Removed by theirs, unchanged on ours: removed.
        let base = [p("a", "1"), p("b", "2")];
        let (out, c) = merge_user_props(&base, &base, &[p("b", "2")]);
        assert_eq!(out, [p("b", "2")]);
        assert!(c.is_empty());
        // Removed by ours, changed by theirs: conflict, ours (removal) kept.
        let (out, c) = merge_user_props(&base, &[p("b", "2")], &[p("a", "9"), p("b", "2")]);
        assert_eq!(out, [p("b", "2")]);
        assert_eq!(c.len(), 1);
        // Added on both with the same value: once. Theirs-only keys appended in theirs order.
        let (out, c) = merge_user_props(
            &[],
            &[p("x", "1")],
            &[p("z", "3"), p("x", "1"), p("y", "2")],
        );
        assert!(c.is_empty());
        assert_eq!(out, [p("x", "1"), p("z", "3"), p("y", "2")]);
    }
}
