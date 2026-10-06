//! Alias closure, linked and unlinked references (BIT-SP-0003.R9, R17; BIT-T-0065, T-0066).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod read_common;

use bitacora_index::{RefFilters, RefGroup, read::sql};
use read_common::indexed;

fn all_titles(groups: &[RefGroup]) -> Vec<(String, String)> {
    groups
        .iter()
        .flat_map(|g| {
            g.blocks
                .iter()
                .map(|h| (g.page.name.clone(), h.block.title.clone()))
        })
        .collect()
}

#[test]
fn inherited_ref_returns_parent_with_child_in_its_subtree() {
    let fx = indexed(&[
        (
            "pages/Log.md",
            "- met [[Ana]]\n  id:: 6650a1b2-0000-4000-8000-0000000000a1\n  - discussed budget\n    - deep note\n- unrelated\n",
        ),
        ("pages/Ana.md", "- [[Ana]] self note\n"),
    ]);
    let groups = fx
        .reader
        .linked_references(fx.page_id("ana"))
        .expect("linked");
    assert_eq!(
        all_titles(&groups),
        [("log".to_owned(), "met [[Ana]]".to_owned())]
    );
    let hit = &groups[0].blocks[0];
    assert!(hit.breadcrumb.is_empty());
    let sub = fx.reader.subtree(&hit.block.uuid).expect("subtree");
    assert_eq!(sub.len(), 3);
    // The child's path-refs include `ana` (view over the interval join).
    let n: i64 = fx
        .index
        .reader()
        .expect("reader")
        .query_row(
            "SELECT count(*) FROM block_path_refs r JOIN blocks b ON b.id = r.block_id \
             WHERE b.title = 'deep note' AND r.page_id = ?1",
            [fx.page_id("ana")],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(n, 1);
}

#[test]
fn nested_matches_fold_into_the_topmost_and_carry_a_breadcrumb() {
    let fx = indexed(&[(
        "pages/Log.md",
        "- parent [[Ana]]\n  - child [[Ana]]\n- other\n  - mid\n    - leaf #ana\n",
    )]);
    let groups = fx
        .reader
        .linked_references(fx.page_id("ana"))
        .expect("linked");
    let titles: Vec<_> = groups[0]
        .blocks
        .iter()
        .map(|h| h.block.title.as_str())
        .collect();
    assert_eq!(titles, ["parent [[Ana]]", "leaf #ana"]);
    let crumbs: Vec<_> = groups[0].blocks[1]
        .breadcrumb
        .iter()
        .map(|c| c.title.as_str())
        .collect();
    assert_eq!(crumbs, ["other", "mid"]);
}

#[test]
fn link_and_tag_forms_are_one_kind_agnostic_ref() {
    let fx = indexed(&[(
        "pages/Log.md",
        "- both [[Zed]] and #zed together\n- tag only #zed\n- bracket tag #[[Zed]]\n- plain [[zed]]\n",
    )]);
    let groups = fx
        .reader
        .linked_references(fx.page_id("zed"))
        .expect("linked");
    assert_eq!(groups[0].blocks.len(), 4, "{:?}", all_titles(&groups));
    // One page, however many spellings.
    let pages: i64 = fx
        .index
        .reader()
        .expect("reader")
        .query_row("SELECT count(*) FROM pages WHERE name = 'zed'", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(pages, 1);
}

#[test]
fn alias_closure_is_symmetric_and_two_hops() {
    let fx = indexed(&[
        ("pages/Ana Lopez.md", "alias:: Ana, A.L.\n\n- profile\n"),
        (
            "pages/Log.md",
            "- called [[Ana]]\n- wrote to [[A.L.]]\n- by name [[Ana Lopez]]\n",
        ),
        ("pages/Far.md", "alias:: Ana Lopez\n\n- far\n"),
    ]);
    let lopez = fx.page_id("ana lopez");
    let ana = fx.page_id("ana");
    let closure = fx.reader.alias_closure(lopez).expect("closure");
    assert!(closure.contains(&ana) && closure.contains(&lopez));
    // Symmetric: from the alias the declaring page is reachable too.
    let back = fx.reader.alias_closure(ana).expect("closure");
    assert!(back.contains(&lopez));
    // Two hops: Far declares Ana Lopez, which declares Ana.
    assert!(back.contains(&fx.page_id("far")));

    let groups = fx.reader.linked_references(lopez).expect("linked");
    let titles: Vec<_> = all_titles(&groups).into_iter().map(|(_, t)| t).collect();
    // `Far` declares `alias:: Ana Lopez`, so its page-properties block references the page too.
    assert_eq!(
        titles,
        [
            "alias:: Ana Lopez",
            "called [[Ana]]",
            "wrote to [[A.L.]]",
            "by name [[Ana Lopez]]"
        ]
    );
    // The alias page redirects to its declaring page when it has no content.
    let redirect = fx.reader.alias_redirect(ana).expect("redirect");
    assert!(redirect.is_some());
    assert!(fx.reader.alias_redirect(lopez).expect("redirect").is_none());
}

#[test]
fn own_page_blocks_are_excluded() {
    let fx = indexed(&[
        ("pages/Ana.md", "- [[Ana]] self note\n"),
        ("pages/Other.md", "- hello [[Ana]]\n"),
    ]);
    let groups = fx
        .reader
        .linked_references(fx.page_id("ana"))
        .expect("linked");
    assert_eq!(
        all_titles(&groups),
        [("other".to_owned(), "hello [[Ana]]".to_owned())]
    );
}

#[test]
fn filters_include_and_exclude_by_path_refs() {
    let fx = indexed(&[
        (
            "pages/Notes.md",
            "- [[meeting]]\n  - talked with [[Ana]]\n- solo [[Ana]]\n- project work [[Ana]] [[project]]\n",
        ),
        (
            "pages/Ana.md",
            "filters:: {\"meeting\" false}\n\n- ana page\n",
        ),
    ]);
    let ana = fx.page_id("ana");
    // The page's own `filters::` property excludes blocks under `meeting`.
    let by_prop = fx.reader.linked_references(ana).expect("linked");
    let t: Vec<_> = all_titles(&by_prop).into_iter().map(|(_, t)| t).collect();
    assert_eq!(t, ["solo [[Ana]]", "project work [[Ana]] [[project]]"]);

    let none = RefFilters::default();
    let all = fx
        .reader
        .linked_references_with(ana, &none)
        .expect("linked");
    assert_eq!(all_titles(&all).len(), 3);

    let only_project = RefFilters {
        include: vec!["Project".into()],
        exclude: vec![],
    };
    let inc = fx
        .reader
        .linked_references_with(ana, &only_project)
        .expect("linked");
    assert_eq!(
        all_titles(&inc),
        [(
            "notes".to_owned(),
            "project work [[Ana]] [[project]]".to_owned()
        )]
    );
    let both = RefFilters::parse(r#"{"meeting" true, "Project" false}"#);
    assert_eq!(both.include, ["meeting"]);
    assert_eq!(both.exclude, ["Project"]);
}

#[test]
fn groups_list_journals_newest_first_then_pages_by_name() {
    let fx = indexed(&[
        ("journals/2026_10_01.md", "- a [[Topic]]\n"),
        ("journals/2026_10_06.md", "- b [[Topic]]\n"),
        ("pages/Zeta.md", "- z [[Topic]]\n"),
        ("pages/Alpha.md", "- a [[Topic]]\n"),
    ]);
    let groups = fx
        .reader
        .linked_references(fx.page_id("topic"))
        .expect("linked");
    let days: Vec<_> = groups.iter().map(|g| g.page.journal_day).collect();
    assert_eq!(days, [Some(20_261_006), Some(20_261_001), None, None]);
    let tail: Vec<_> = groups[2..].iter().map(|g| g.page.name.as_str()).collect();
    assert_eq!(tail, ["alpha", "zeta"]);
}

#[test]
fn unlinked_references_follow_the_logseq_regex() {
    let fx = indexed(&[
        ("pages/Ana.md", "- Ana is here\n"),
        (
            "pages/Log.md",
            "- call Ana tomorrow\n- call [[Ana]] tomorrow\n- Anaconda setup\n- tagged #Ana\n- Ana\n- (ana)\n- call Ana and [[Ana]]\n- ana's thing\n",
        ),
        (
            "pages/Logbook.md",
            "- task Ana\n  :LOGBOOK:\n  CLOCK: [2026-10-06 Tue 10:00]\n  :END:\n- only logged\n  :LOGBOOK:\n  Ana in drawer\n  :END:\n",
        ),
    ]);
    let groups = fx
        .reader
        .unlinked_references(fx.page_id("ana"))
        .expect("unlinked");
    let got: Vec<_> = all_titles(&groups);
    let log: Vec<&str> = got
        .iter()
        .filter(|(p, _)| p == "log")
        .map(|(_, t)| t.as_str())
        .collect();
    assert_eq!(
        log,
        ["call Ana tomorrow", "Ana", "(ana)", "ana's thing"],
        "{got:?}"
    );
    let logbook: Vec<&str> = got
        .iter()
        .filter(|(p, _)| p == "logbook")
        .map(|(_, t)| t.as_str())
        .collect();
    assert_eq!(logbook, ["task Ana"]);
    assert!(got.iter().all(|(p, _)| p != "ana"));
}

#[test]
fn unlinked_references_cover_aliases() {
    let fx = indexed(&[
        ("pages/Ana Lopez.md", "alias:: Annie\n\n- profile\n"),
        ("pages/Log.md", "- saw Annie today\n- saw Ana Lopez too\n"),
    ]);
    let groups = fx
        .reader
        .unlinked_references(fx.page_id("ana lopez"))
        .expect("unlinked");
    assert_eq!(all_titles(&groups).len(), 2);
}

#[test]
fn unlinked_prefilter_goes_through_fts() {
    // Enough rows that the planner prefers seeking by rowid over scanning `blocks`.
    let filler: String = (0..400).map(|i| format!("- filler {i}\n")).collect();
    let fx = indexed(&[("pages/Ana.md", "- x\n"), ("pages/Filler.md", &filler)]);
    let conn = fx.index.reader().expect("reader");
    let q = format!(
        "EXPLAIN QUERY PLAN {}",
        sql::UNLINKED_FTS_SQL.replace("{COLS}", "b.id")
    );
    let mut st = conn.prepare(&q).expect("prepare");
    let plan: Vec<String> = st
        .query_map(rusqlite::params!["\"ana\"", 1_i64], |r| {
            r.get::<_, String>(3)
        })
        .expect("plan")
        .map(|x| x.expect("row"))
        .collect();
    let plan = plan.join("\n");
    assert!(plan.contains("VIRTUAL TABLE INDEX"), "{plan}");
    assert!(!plan.contains("SCAN b"), "{plan}");
    assert!(
        plan.contains("SEARCH b USING INTEGER PRIMARY KEY"),
        "{plan}"
    );
}

#[test]
fn every_page_of_the_docs_graph_answers_reference_queries() {
    use bitacora_config::EffectiveConfig;
    use bitacora_index::{Indexer, IndexerOptions, PageFilter, PageSort};

    let graph = bitacora_testkit::graph("logseq-docs");
    let env = read_common::common::env_for(graph.clone());
    let index = env.open();
    let ix = Indexer::start(
        &index,
        IndexerOptions::new(&graph, EffectiveConfig::default()),
    )
    .expect("indexer");
    ix.reconcile().expect("reconcile");
    ix.shutdown();
    let r = index.read_api();
    let all = PageFilter {
        journals: true,
        placeholders: true,
        builtins: true,
    };
    let pages = r.all_pages(all, PageSort::Name).expect("pages");
    assert!(pages.len() > 100);
    let mut with_refs = 0;
    for p in pages.iter().take(150) {
        let linked = r.linked_references(p.id).expect("linked");
        assert!(
            linked
                .iter()
                .all(|g| g.page.id != p.id && !g.blocks.is_empty())
        );
        with_refs += usize::from(!linked.is_empty());
        let unlinked = r.unlinked_references(p.id).expect("unlinked");
        assert!(unlinked.iter().all(|g| g.page.id != p.id));
    }
    assert!(
        with_refs > 10,
        "only {with_refs} pages have linked references"
    );
}
