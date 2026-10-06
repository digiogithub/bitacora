//! Outline, subtree, ancestors and page lookups (BIT-SP-0003.R4, BIT-T-0064).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod read_common;

use bitacora_index::{PageFilter, PageRow, PageSort, read::sql};
use read_common::{Fixture, indexed, titles};

const A: &str = "6650a1b2-0000-4000-8000-00000000000a";
const C: &str = "6650a1b2-0000-4000-8000-00000000000c";

fn tree(collapsed: bool) -> String {
    let c = if collapsed {
        "  collapsed:: true\n"
    } else {
        ""
    };
    format!("- A\n  id:: {A}\n{c}  - B\n    - C\n      id:: {C}\n- D\n")
}

#[test]
fn subtree_returns_block_and_descendants_only() {
    let fx = indexed(&[("pages/Tree.md", &tree(false))]);
    let sub = fx.reader.subtree(A).expect("subtree");
    assert_eq!(titles(&sub), ["A", "B", "C"]);
    assert!(sub.windows(2).all(|w| w[0].ord < w[1].ord));
    // Properties come back in file order, with the raw key.
    assert_eq!(sub[0].properties, [("id".to_owned(), A.to_owned())]);
    let none = fx
        .reader
        .subtree("00000000-0000-4000-8000-000000000000")
        .expect("none");
    assert!(none.is_empty());
}

#[test]
fn ancestors_are_the_breadcrumb_outermost_first() {
    let fx = indexed(&[("pages/Tree.md", &tree(false))]);
    assert_eq!(titles(&fx.reader.ancestors(C).expect("anc")), ["A", "B"]);
    assert!(fx.reader.ancestors(A).expect("anc").is_empty());
    let c = fx.reader.block(C).expect("block").expect("some");
    assert_eq!((c.title.as_str(), c.depth), ("C", 3));
    let missing = fx
        .reader
        .block("6650a1b2-0000-4000-8000-0000000000ff")
        .expect("none");
    assert!(missing.is_none());
}

#[test]
fn collapsed_subtree_is_skipped_only_on_request() {
    let fx = indexed(&[("pages/Tree.md", &tree(true))]);
    let page = fx.page_id("tree");
    let visible = fx.reader.outline(page, 0, 100, true).expect("outline");
    assert_eq!(titles(&visible), ["A", "D"]);
    assert!(visible[0].collapsed);
    let all = fx.reader.outline(page, 0, 100, false).expect("outline");
    assert_eq!(titles(&all), ["A", "B", "C", "D"]);
}

#[test]
fn outline_paginates_in_order() {
    let mut text = String::new();
    for i in 0..80 {
        text.push_str(&format!("- item {i:02}\n"));
    }
    let fx = indexed(&[("pages/Long.md", &text)]);
    let page = fx.page_id("long");
    let first = fx.reader.outline(page, 0, 50, true).expect("first");
    assert_eq!(first.len(), 50);
    let next = fx.reader.outline(page, 50, 25, true).expect("next");
    let want: Vec<String> = (50..75).map(|i| format!("item {i:02}")).collect();
    assert_eq!(titles(&next), want);
    let tail = fx.reader.outline(page, 75, 25, true).expect("tail");
    assert_eq!(tail.len(), 5);
}

fn names(v: Vec<PageRow>) -> Vec<String> {
    v.into_iter().map(|p| p.name).collect()
}

#[test]
fn page_lookups_and_listings() {
    let fx = indexed(&[
        ("pages/Alpha.md", "- a\n"),
        ("pages/Beta.md", "- b\n- see [[Ghost]]\n"),
        ("journals/2026_10_01.md", "- j1\n"),
        ("journals/2026_10_06.md", "- j2\n"),
        ("journals/2026_09_15.md", "- j3\n"),
    ]);
    let r = &fx.reader;
    let alpha = r.page_by_name("ALPHA").expect("q").expect("found");
    assert_eq!(alpha.original_name, "Alpha");
    assert_eq!(alpha.file_path.as_deref(), Some("pages/Alpha.md"));
    let by_uuid = r.page_by_uuid(&alpha.uuid).expect("q");
    assert_eq!(by_uuid.map(|p| p.id), Some(alpha.id));
    let by_id = r.page_by_id(alpha.id).expect("q");
    assert_eq!(by_id.map(|p| p.name), Some("alpha".to_owned()));
    let ghost = r.page_by_name("ghost").expect("q").expect("placeholder");
    assert!(ghost.is_placeholder());
    assert!(r.page_by_name("nope").expect("q").is_none());

    let no_journals = PageFilter {
        journals: false,
        ..PageFilter::default()
    };
    let listed = r.all_pages(no_journals, PageSort::Name).expect("pages");
    assert_eq!(names(listed), ["alpha", "beta"]);
    let all = PageFilter {
        journals: true,
        placeholders: true,
        builtins: false,
    };
    let listed = r.all_pages(all, PageSort::Name).expect("pages");
    assert_eq!(listed.len(), 6, "{:?}", names(listed.clone()));
    assert_eq!(listed.iter().filter(|p| p.is_journal).count(), 3);

    let days = |v: Vec<PageRow>| v.iter().map(|p| p.journal_day).collect::<Vec<_>>();
    let newest = r.journals(None, 10).expect("j");
    assert_eq!(
        days(newest),
        [Some(20_261_006), Some(20_261_001), Some(20_260_915)]
    );
    let older = r.journals(Some(20_261_006), 1).expect("j");
    assert_eq!(days(older), [Some(20_261_001)]);
}

fn plan(fx: &Fixture, sql: &str, n_params: usize) -> String {
    let conn = fx.index.reader().expect("reader");
    let q = format!("EXPLAIN QUERY PLAN {}", sql.replace("{COLS}", "b.id"));
    let mut st = conn.prepare(&q).expect("prepare");
    let one = 1_i64;
    let params: Vec<&dyn rusqlite::ToSql> = (0..n_params)
        .map(|_| &one as &dyn rusqlite::ToSql)
        .collect();
    st.query_map(params.as_slice(), |r| r.get::<_, String>(3))
        .expect("plan")
        .map(|x| x.expect("row"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn outline_and_subtree_use_the_block_indexes() {
    let fx = indexed(&[("pages/Tree.md", &tree(false))]);
    let outline = plan(&fx, sql::OUTLINE_SQL, 4);
    assert!(outline.contains("blocks_page"), "{outline}");
    let subtree = plan(&fx, sql::SUBTREE_SQL, 1);
    assert!(
        subtree.contains("sqlite_autoindex_blocks") || subtree.contains("blocks_page"),
        "{subtree}"
    );
    assert!(!subtree.contains("SCAN b"), "{subtree}");
}
