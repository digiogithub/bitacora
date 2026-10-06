//! Golden merge matrix for `merge_page` (BIT-US-0051). All vectors are our own.

use bitacora_merge::{ConflictKind, MergeEnv, NoteKind, merge_page};
use proptest::prelude::*;

const ID1: &str = "11110000-0000-4000-8000-000000000001";

fn m(b: &str, o: &str, t: &str) -> bitacora_merge::MergeResult {
    let r = merge_page(b, o, t, &MergeEnv::new());
    assert!(!r.output.contains("<<<<<<<"), "markers in output");
    assert!(!r.output.contains(">>>>>>>"), "markers in output");
    r
}

#[test]
fn both_insert_same_place_ours_first() {
    let b = "- a\n- c\n";
    let o = "- a\n- from ours\n- c\n";
    let t = "- a\n- from theirs\n- c\n";
    let r = m(b, o, t);
    assert_eq!(r.output, "- a\n- from ours\n- from theirs\n- c\n");
    assert!(r.conflicts.is_empty());
}

#[test]
fn both_insert_identical_block_is_deduped() {
    let r = m(
        "- a\n",
        "- a\n- new thing here today ok\n",
        "- a\n- new thing here today ok\n",
    );
    assert_eq!(r.output, "- a\n- new thing here today ok\n");
}

#[test]
fn insert_on_one_side_only() {
    let r = m("- a\n- b\n", "- a\n- b\n", "- a\n- x\n- b\n");
    assert_eq!(r.output, "- a\n- x\n- b\n");
    let r = m("- a\n- b\n", "- a\n- x\n- b\n  \n", "- a\n- b\n- y\n");
    assert!(r.output.contains("- x\n"));
    assert!(r.output.contains("- y"));
}

#[test]
fn one_side_delete_of_unchanged_block() {
    let r = m(
        "- a\n- b\n- c\n",
        "- a\n- c\n",
        "- a\n- b\n- c changed text\n",
    );
    assert_eq!(r.output, "- a\n- c changed text\n");
    assert!(r.conflicts.is_empty());
}

#[test]
fn delete_on_both_sides() {
    let r = m("- a\n- b\n- c\n", "- a\n- c\n", "- a\n- c\n  x:: 1\n");
    assert_eq!(r.output, "- a\n- c\n  x:: 1\n");
}

#[test]
fn delete_vs_modify_is_a_conflict_and_keeps_the_modified_block() {
    let b = "- a\n- the block to remove or edit\n- c\n";
    let o = "- a\n- c\n";
    let t = "- a\n- the block to remove or edit today\n- c\n";
    let r = m(b, o, t);
    assert_eq!(r.output, "- a\n- the block to remove or edit today\n- c\n");
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(r.conflicts[0].conflict.kind, ConflictKind::DeleteVsModify);
    assert_eq!(r.conflicts[0].conflict.ours, None);
    // symmetric
    let r = m(b, t, o);
    assert_eq!(r.conflicts.len(), 1);
    assert!(r.conflicts[0].conflict.theirs.is_none());
    assert!(r.output.contains("today"));
}

#[test]
fn deleted_parent_with_modified_child_conflicts_at_the_child_and_reattaches() {
    let b = "- keep\n- parent block title here\n  - child block title here now\n";
    let o = "- keep\n";
    let t = "- keep\n- parent block title here\n  - child block title here now changed\n";
    let r = m(b, o, t);
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(r.conflicts[0].conflict.kind, ConflictKind::DeleteVsModify);
    assert!(r.notes.iter().any(|n| n.kind == NoteKind::Reattached));
    assert_eq!(r.output, "- keep\n- child block title here now changed\n");
}

#[test]
fn new_child_under_deleted_parent_is_reattached() {
    let b = "- keep\n- parent block title here\n";
    let o = "- keep\n";
    let t = "- keep\n- parent block title here\n  - brand new child under it\n";
    let r = m(b, o, t);
    assert!(r.conflicts.is_empty());
    assert_eq!(r.output, "- keep\n- brand new child under it\n");
    assert!(r.notes.iter().any(|n| n.kind == NoteKind::Reattached));
}

#[test]
fn reorder_on_one_side_is_applied() {
    let b = "- one a b c\n- two a b c\n- three a b c\n";
    let r = m(b, b, "- three a b c\n- one a b c\n- two a b c\n");
    assert_eq!(r.output, "- three a b c\n- one a b c\n- two a b c\n");
    let r = m(
        b,
        "- two a b c\n- one a b c\n- three a b c\n",
        "- one a b c\n- two a b c\n- three a b c\n  x:: 1\n",
    );
    assert_eq!(
        r.output,
        "- two a b c\n- one a b c\n- three a b c\n  x:: 1\n"
    );
    assert!(r.conflicts.is_empty());
}

#[test]
fn competing_reorders_keep_ours_with_a_note() {
    let b = "- one a b c\n- two a b c\n- three a b c\n";
    let o = "- two a b c\n- one a b c\n- three a b c\n";
    let t = "- three a b c\n- two a b c\n- one a b c\n";
    let r = m(b, o, t);
    assert_eq!(r.output, o);
    assert!(r.conflicts.is_empty());
    assert!(r.notes.iter().any(|n| n.kind == NoteKind::CompetingReorder));
}

#[test]
fn move_under_other_parent_on_one_side() {
    let b = "- alpha block header here\n  - kid one of alpha\n- beta block header here\n";
    let t = "- alpha block header here\n- beta block header here\n  - kid one of alpha\n";
    let o =
        "- alpha block header here\n  - kid one of alpha\n- beta block header here\n  extra:: 1\n";
    let r = m(b, o, t);
    assert_eq!(
        r.output,
        "- alpha block header here\n- beta block header here\n  extra:: 1\n  - kid one of alpha\n"
    );
    assert!(r.conflicts.is_empty());
}

#[test]
fn competing_moves_take_ours_with_a_note() {
    let b = "- alpha block header here\n- beta block header here\n- gamma block header here\n- the wanderer block text\n";
    let o = "- alpha block header here\n  - the wanderer block text\n- beta block header here\n- gamma block header here\n";
    let t = "- alpha block header here\n- beta block header here\n- gamma block header here\n  - the wanderer block text\n";
    let r = m(b, o, t);
    assert_eq!(r.output, o);
    assert!(r.notes.iter().any(|n| n.kind == NoteKind::CompetingMove));
    assert!(r.conflicts.is_empty());
}

#[test]
fn metadata_only_change_edits_the_block_surgically() {
    let b = "- Topic a b c\n  note:: keep   spacing\n- other   block  \n";
    let o = "- Topic a b c\n  note:: keep   spacing\n  collapsed:: true\n- other   block  \n";
    let t = "- Topic a b c\n  note:: keep   spacing\n- other   block  \n\t- child\n";
    let r = m(b, o, t);
    assert_eq!(
        r.output,
        "- Topic a b c\n  note:: keep   spacing\n  collapsed:: true\n- other   block  \n\t- child\n"
    );
}

#[test]
fn theirs_content_ours_collapsed_combine_with_ours_bytes_untouched() {
    let b = "- Topic is here now\n- Second  \n";
    let o = "- Topic is here now\n  collapsed:: true\n- Second  \n";
    let t = "- Topic is here now (v2)\n- Second  \n";
    let r = m(b, o, t);
    assert!(r.conflicts.is_empty());
    assert_eq!(
        r.output,
        "- Topic is here now (v2)\n  collapsed:: true\n- Second  \n"
    );
}

#[test]
fn scheduled_is_reemitted_after_the_title() {
    let b = "- TODO write the final report\n  SCHEDULED: <2026-10-06 Tue>\n  note:: a\n";
    let o = "- TODO write the final report\n  SCHEDULED: <2026-10-06 Tue>\n  note:: a\n  card-repeats:: 2\n";
    let t = "- DONE write the final report\n  SCHEDULED: <2026-10-06 Tue>\n  note:: a\n";
    let r = m(b, o, t);
    assert!(r.conflicts.is_empty());
    assert_eq!(
        r.output,
        "- DONE write the final report\n  SCHEDULED: <2026-10-06 Tue>\n  note:: a\n  card-repeats:: 2\n"
    );
}

#[test]
fn logbook_union_rebuilds_block() {
    let c1 = "CLOCK: [2026-10-05 Mon 09:00:00]--[2026-10-05 Mon 10:00:00] =>  01:00:00";
    let c2 = "CLOCK: [2026-10-05 Mon 11:00:00]--[2026-10-05 Mon 12:00:00] =>  01:00:00";
    let b = "- t\n";
    let o = format!("- t\n  :LOGBOOK:\n  {c2}\n  :END:\n");
    let t = format!("- t\n  :LOGBOOK:\n  {c1}\n  :END:\n");
    let r = m(b, &o, &t);
    assert_eq!(
        r.output,
        format!("- t\n  :LOGBOOK:\n  {c1}\n  {c2}\n  :END:\n")
    );
}

#[test]
fn id_rewrites_apply_across_the_page() {
    let (a, bb) = (ID1, "22220000-0000-4000-8000-000000000002");
    let b = "- Idea\n- ref to ((x))\n".replace("x", a);
    let o = format!("- Idea\n  id:: {a}\n- ref to (({a}))\n");
    let t = format!("- Idea\n  id:: {bb}\n- ref to (({a}))\n");
    let referenced = |id: &str| id == bb;
    let env = MergeEnv {
        is_referenced: &referenced,
        ..MergeEnv::new()
    };
    let r = merge_page(&b, &o, &t, &env);
    assert!(r.output.contains(&format!("id:: {bb}")));
    assert!(r.output.contains(&format!("(({bb}))")));
    assert!(!r.output.contains(&format!("(({a}))")));
}

#[test]
fn crlf_bom_and_tabs_of_ours_are_kept() {
    let b = "\u{feff}- a\r\n\t- b\r\n";
    let o = "\u{feff}- a\r\n\t- b\r\n\t- ours new block text\r\n";
    let t = "\u{feff}- a\r\n\t- b\r\n- entirely different words zzz\r\n";
    let r = m(b, o, t);
    assert_eq!(
        r.output,
        "\u{feff}- a\r\n\t- b\r\n\t- ours new block text\r\n- entirely different words zzz\r\n"
    );
}

#[test]
fn theirs_block_is_written_in_our_indent_style() {
    let b = "- a\n    - b\n";
    let o = "- a\n    - b\n";
    let t = "- a\n  - b\n  - c\n";
    // ours == base short-circuits to theirs verbatim.
    assert_eq!(m(b, o, t).output, t);
    let o2 = "- a\n    - b\n    - ours\n";
    let r = m(b, o2, t);
    assert_eq!(r.output, "- a\n    - b\n    - ours\n    - c\n");
}

#[test]
fn page_properties_merge_per_key() {
    let b = "title:: P\nalias:: x\n\n- a\n";
    let o = "title:: P\nalias:: x\nicon:: 1\n\n- a\n";
    let t = "title:: P\nalias:: y\n\n- a\n- b\n";
    let r = m(b, o, t);
    assert!(r.conflicts.is_empty());
    assert!(r.output.contains("alias:: y"));
    assert!(r.output.contains("icon:: 1"));
    assert!(r.output.ends_with("- a\n- b\n"));
}

#[test]
fn content_conflict_holds_ours() {
    let b = "- same line here ok\n";
    let o = "- same line here ok ours\n- x\n";
    let t = "- same line here ok theirs\n";
    let r = m(b, o, t);
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(r.conflicts[0].conflict.kind, ConflictKind::Content);
    assert_eq!(r.output, o);
}

#[test]
fn deterministic_for_duplicate_ids() {
    let src = format!("- a\n  id:: {ID1}\n- b\n  id:: {ID1}\n");
    let p1 = bitacora_merge::MergePage::parse(&src);
    let p2 = bitacora_merge::MergePage::parse(&src);
    assert_eq!(p1.blocks[1].fresh_id, p2.blocks[1].fresh_id);
}

fn page() -> impl Strategy<Value = String> {
    let line = prop::sample::select(vec![
        "- alpha beta gamma delta\n",
        "- TODO task one two three\n",
        "- note:: x\n",
        "- tail   spaces  \n",
        "\t- child one two three four\n",
        "\t\t- grand child text here ok\n",
        "- gamma\n  collapsed:: true\n",
        "- with prop block text here\n  owner:: ana\n",
    ]);
    prop::collection::vec(line, 0..8).prop_map(|v| v.concat())
}

proptest! {
    #[test]
    fn identity_laws(b in page(), x in page()) {
        let env = MergeEnv::new();
        prop_assert_eq!(merge_page(&b, &x, &x, &env).output, x.clone());
        prop_assert_eq!(merge_page(&b, &b, &x, &env).output, x.clone());
        prop_assert_eq!(merge_page(&b, &x, &b, &env).output, x);
    }

    #[test]
    fn never_emits_markers_and_is_deterministic(b in page(), o in page(), t in page()) {
        let env = MergeEnv::new();
        let r1 = merge_page(&b, &o, &t, &env);
        let r2 = merge_page(&b, &o, &t, &env);
        prop_assert_eq!(&r1, &r2);
        prop_assert!(!r1.output.contains("<<<<<<<"));
    }

    #[test]
    fn appends_on_both_sides_keep_every_original_byte(b in page()) {
        let env = MergeEnv::new();
        let o = format!("{b}- ours qqq www eee rrr\n");
        let t = format!("{b}- theirs ttt yyy uuu iii\n");
        let r = merge_page(&b, &o, &t, &env);
        prop_assert_eq!(r.output, format!("{b}- ours qqq www eee rrr\n- theirs ttt yyy uuu iii\n"));
        prop_assert!(r.conflicts.is_empty());
    }
}

#[test]
fn identity_laws_over_fixture_pages() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/graphs");
    let mut n = 0;
    let mut stack = vec![root];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "md") {
                let Ok(text) = std::fs::read_to_string(&p) else {
                    continue;
                };
                let env = MergeEnv::new();
                let edited = format!("{text}\n- appended block for the law test\n");
                assert_eq!(merge_page(&text, &edited, &edited, &env).output, edited);
                assert_eq!(merge_page(&text, &text, &edited, &env).output, edited);
                assert_eq!(merge_page(&text, &edited, &text, &env).output, edited);
                // Both sides append different things: the original prefix survives byte for byte.
                let a = format!("{text}\n- ours appended text here now\n");
                let c = format!("{text}\n- theirs different zzz yyy\n");
                let r = merge_page(&text, &a, &c, &env);
                assert!(
                    r.output.starts_with(text.trim_end_matches('\n')),
                    "{}",
                    p.display()
                );
                n += 1;
            }
        }
    }
    assert!(n > 20, "fixtures found: {n}");
}

#[test]
fn spec_r12_scenarios_with_short_blocks() {
    let r = m("- A\n- C\n", "- A\n- B1\n- C\n", "- A\n- B2\n- C\n");
    assert_eq!(r.output, "- A\n- B1\n- B2\n- C\n");

    let r = m("- Draft\n", "", "- Draft v2\n");
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(r.conflicts[0].conflict.kind, ConflictKind::DeleteVsModify);
    assert_eq!(r.output, "- Draft v2\n");

    let r = m("- A\n- B\n", "- B\n- A\n", "- A!\n- B\n");
    assert_eq!(r.output, "- B\n- A!\n");
    assert!(r.conflicts.is_empty());
}

#[test]
fn only_changed_blocks_differ_with_tabs_and_crlf() {
    let blocks: Vec<String> = (0..40)
        .map(|i| format!("\t- block number {i} text here\r\n"))
        .collect();
    let b = format!("- root\r\n{}", blocks.concat());
    let mut ob = blocks.clone();
    ob[3] = "\t- block number 3 text here ours\r\n".to_owned();
    let o = format!("- root\r\n{}", ob.concat());
    let mut tb = blocks;
    tb[7] = "\t- block number 7 text here theirs\r\n".to_owned();
    let t = format!("- root\r\n{}", tb.concat());
    let r = m(&b, &o, &t);
    let expect = o.replace(
        "block number 7 text here\r\n",
        "block number 7 text here theirs\r\n",
    );
    assert_eq!(r.output, expect);
}
