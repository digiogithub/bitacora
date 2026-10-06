//! Field-level merge of one matched block (`docs/design/git-sync-merge.md` §4.3).
//!
//! Content, user properties and planning lines merge three-way and may conflict; metadata
//! resolves automatically (ADR-009). A conflicting field holds the **ours** value in the output.

use crate::conflict::{Conflict, IdRewrite};
use crate::fields::{merge_content, merge_user_props};
use crate::marker::merge_planning;
use crate::meta::{MergeEnv, merge_meta};
use crate::model::{MergeBlock, Meta, PropEntry};

/// A block after field-level merging, ready for serialization by the structural merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedBlock {
    /// Merged content (first line with marker plus continuation lines).
    pub content: String,
    /// Merged `SCHEDULED` / `DEADLINE` lines.
    pub planning: Vec<(String, String)>,
    /// Merged content-class properties.
    pub props: Vec<PropEntry>,
    /// Merged metadata.
    pub meta: Meta,
    /// Order of every property key in the output: ours, then keys only theirs has.
    pub prop_order: Vec<String>,
    /// Content and property conflicts (never metadata).
    pub conflicts: Vec<Conflict>,
    /// `((from))` references to rewrite to `((to))` across the merge result.
    pub id_rewrites: Vec<IdRewrite>,
}

/// Merges one matched block. `base` is `None` when both sides added the block.
#[must_use]
pub fn merge_block(
    base: Option<&MergeBlock>,
    ours: &MergeBlock,
    theirs: &MergeBlock,
    env: &MergeEnv<'_>,
) -> MergedBlock {
    let mut conflicts = Vec::new();

    let content = merge_content(
        base.map(|b| b.content.as_str()),
        &ours.content,
        &theirs.content,
    );
    conflicts.extend(content.conflict);

    let none: &[PropEntry] = &[];
    let (props, c) = merge_user_props(
        base.map_or(none, |b| b.props.as_slice()),
        &ours.props,
        &theirs.props,
    );
    conflicts.extend(c);

    let none_pl: &[(String, String)] = &[];
    let (planning, c) = merge_planning(
        base.map_or(none_pl, |b| b.planning.as_slice()),
        &ours.planning,
        &theirs.planning,
    );
    conflicts.extend(c);

    let (meta, id_rewrites) = merge_meta(base.map(|b| &b.meta), &ours.meta, &theirs.meta, env);

    // Property order: ours, then keys only theirs has; only keys that survive the merge.
    let mut present: Vec<&str> = props.iter().map(|p| p.norm.as_str()).collect();
    present.extend(meta.collapsed.as_ref().map(|_| "collapsed"));
    present.extend(meta.id.as_ref().map(|_| "id"));
    present.extend(meta.card.iter().map(|(k, _)| k.as_str()));
    present.extend(meta.lww.iter().map(|(k, _)| k.as_str()));
    let mut prop_order: Vec<String> = Vec::new();
    for k in ours.prop_order.iter().chain(theirs.prop_order.iter()) {
        if present.contains(&k.as_str()) && !prop_order.contains(k) {
            prop_order.push(k.clone());
        }
    }
    // Keys that exist only through the merge (e.g. an id taken from the winner) still need a slot.
    for k in present {
        if !prop_order.iter().any(|p| p == k) {
            prop_order.push(k.to_owned());
        }
    }

    MergedBlock {
        content: content.value,
        planning,
        props,
        meta,
        prop_order,
        conflicts,
        id_rewrites,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflict::ConflictKind;
    use crate::model::MergePage;
    use bitacora_markdown::Side;

    const A: &str = "aaaa0000-0000-4000-8000-000000000001";
    const B: &str = "bbbb0000-0000-4000-8000-000000000002";

    fn blk(src: &str) -> MergeBlock {
        MergePage::parse(src).blocks.remove(0)
    }

    fn merge(b: &str, o: &str, t: &str) -> MergedBlock {
        merge_block(Some(&blk(b)), &blk(o), &blk(t), &MergeEnv::new())
    }

    #[test]
    fn r11_collapsed_vs_content() {
        let m = merge(
            "- Topic\n",
            "- Topic\n  collapsed:: true\n",
            "- Topic (v2)\n",
        );
        assert_eq!(m.content, "Topic (v2)");
        assert_eq!(m.meta.collapsed.as_deref(), Some("true"));
        assert!(m.conflicts.is_empty());
        assert_eq!(m.prop_order, ["collapsed"]);
    }

    #[test]
    fn r11_logbook_union_sorted() {
        let c1 = "CLOCK: [2026-10-05 Mon 09:00:00]--[2026-10-05 Mon 10:00:00] =>  01:00:00";
        let c2 = "CLOCK: [2026-10-05 Mon 11:00:00]--[2026-10-05 Mon 12:00:00] =>  01:00:00";
        let m = merge(
            "- t\n",
            &format!("- t\n  :LOGBOOK:\n  {c2}\n  :END:\n"),
            &format!("- t\n  :LOGBOOK:\n  {c1}\n  :END:\n"),
        );
        assert_eq!(m.meta.logbook, [c1, c2]);
        assert!(m.conflicts.is_empty());
    }

    #[test]
    fn r11_card_group_from_later_review() {
        let o = "- c #card\n  card-last-reviewed:: 2026-10-01T08:00:00.000Z\n  card-repeats:: 3\n";
        let t = "- c #card\n  card-last-reviewed:: 2026-10-04T08:00:00.000Z\n  card-repeats:: 4\n";
        let m = merge("- c #card\n", o, t);
        assert_eq!(
            m.meta.card,
            [
                (
                    "card-last-reviewed".to_owned(),
                    "2026-10-04T08:00:00.000Z".to_owned()
                ),
                ("card-repeats".to_owned(), "4".to_owned())
            ]
        );
    }

    #[test]
    fn r11_both_added_different_ids_referenced_wins_and_rewrites() {
        let refd = |id: &str| id == B;
        let env = MergeEnv {
            is_referenced: &refd,
            ..MergeEnv::new()
        };
        let m = merge_block(
            Some(&blk("- Idea\n")),
            &blk(&format!("- Idea\n  id:: {A}\n")),
            &blk(&format!("- Idea\n  id:: {B}\n")),
            &env,
        );
        assert_eq!(m.meta.id.as_deref(), Some(B));
        assert_eq!(m.id_rewrites.len(), 1);
        assert_eq!(m.id_rewrites[0].from, A);
        assert_eq!(m.prop_order, ["id"]);
    }

    #[test]
    fn r10_property_per_key_inside_a_block() {
        let m = merge(
            "- t\n  status:: open\n",
            "- t\n  status:: open\n  owner:: ana\n",
            "- t\n  status:: done\n",
        );
        assert!(m.conflicts.is_empty());
        let got: Vec<_> = m
            .props
            .iter()
            .map(|p| (p.norm.as_str(), p.value.as_str()))
            .collect();
        assert_eq!(got, [("status", "done"), ("owner", "ana")]);
        assert_eq!(m.prop_order, ["status", "owner"]);
    }

    #[test]
    fn r13_scheduled_conflict_and_marker_combination() {
        let b = "- TODO write\n  SCHEDULED: <2026-10-06 Tue>\n";
        let o = "- DONE write\n  SCHEDULED: <2026-10-07 Wed>\n";
        let t = "- TODO write final\n  SCHEDULED: <2026-10-08 Thu>\n";
        let m = merge(b, o, t);
        assert_eq!(m.content, "DONE write final");
        assert_eq!(m.conflicts.len(), 1);
        assert_eq!(m.conflicts[0].kind, ConflictKind::Property);
        assert_eq!(m.conflicts[0].field, "SCHEDULED");
        assert_eq!(m.planning[0].1, "<2026-10-07 Wed>");
    }

    #[test]
    fn metadata_never_conflicts_even_when_everything_differs() {
        let b = "- t\n  collapsed:: false\n  query-table:: true\n";
        let o = "- t\n  collapsed:: true\n  query-table:: false\n  hl-page:: 1\n";
        let t = "- t\n  collapsed:: false\n  query-table:: maybe\n  hl-page:: 2\n";
        let m = merge_block(
            Some(&blk(b)),
            &blk(o),
            &blk(t),
            &MergeEnv {
                prefer: Side::Theirs,
                ..MergeEnv::new()
            },
        );
        assert!(m.conflicts.is_empty());
        assert_eq!(m.meta.collapsed.as_deref(), Some("true"));
        assert_eq!(
            m.meta.lww[0],
            ("query-table".to_owned(), "maybe".to_owned())
        );
    }

    #[test]
    fn both_added_without_base_identical_is_clean() {
        let o = blk("- new block\n  owner:: ana\n");
        let m = merge_block(None, &o, &o.clone(), &MergeEnv::new());
        assert!(m.conflicts.is_empty());
        assert_eq!(m.content, "new block");
    }

    #[test]
    fn whitespace_only_difference_is_not_a_conflict() {
        let m = merge("- a b\n", "- a b  \n  x:: 1\n", "- a b\r\n  x:: 1\r\n");
        assert!(m.conflicts.is_empty());
        assert_eq!(m.content, "a b");
    }

    mod props {
        use super::*;
        use proptest::prelude::*;

        const LINES: &[&str] = &[
            "  collapsed:: true",
            "  owner:: [[Ana]]",
            "  card-repeats:: 2",
            "  SCHEDULED: <2026-10-06 Tue>",
            "  extra line",
            "  query-table:: true",
        ];

        fn block() -> impl Strategy<Value = String> {
            (
                proptest::sample::select(&["- TODO a b", "- a b c", "- DONE a b"][..]),
                proptest::collection::vec(proptest::sample::select(LINES), 0..5),
            )
                .prop_map(|(t, ls)| format!("{t}\n{}\n", ls.join("\n")))
        }

        proptest! {
            #[test]
            fn merge_is_deterministic_and_idempotent(b in block(), o in block(), t in block()) {
                let (bb, ob, tb) = (blk(&b), blk(&o), blk(&t));
                let env = MergeEnv::new();
                let m1 = merge_block(Some(&bb), &ob, &tb, &env);
                let m2 = merge_block(Some(&bb), &ob, &tb, &env);
                prop_assert_eq!(&m1, &m2);
                let same = merge_block(Some(&ob), &ob, &ob, &env);
                prop_assert!(same.conflicts.is_empty());
                prop_assert_eq!(&same.content, &ob.content);
                prop_assert_eq!(&same.meta, &ob.meta);
                // Metadata never produces conflicts.
                prop_assert!(m1.conflicts.iter().all(|c| matches!(c.kind, ConflictKind::Content | ConflictKind::Property)));
            }
        }
    }
}
