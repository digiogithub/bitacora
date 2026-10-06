//! Block refs, tasks/agenda, namespaces and graph edges (BIT-SP-0003.R4, R8; BIT-T-0067).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod read_common;

use bitacora_index::{AgendaKind, GraphEdgeKind, GraphOptions, TaskFilter};
use read_common::{indexed, titles};

const TARGET: &str = "6650a1b2-0000-4000-8000-000000000001";
const DANGLING: &str = "6650a1b2-0000-4000-8000-0000000000ee";

#[test]
fn block_refs_count_list_and_resolve() {
    let fx = indexed(&[
        (
            "pages/Src.md",
            &format!(
                "- the target\n  id:: {TARGET}\n- see (({TARGET}))\n- dangling (({DANGLING}))\n"
            ),
        ),
        ("pages/Other.md", &format!("- {{{{embed (({TARGET}))}}}}\n")),
    ]);
    let r = &fx.reader;
    assert_eq!(r.block_ref_count(TARGET).expect("count"), 2);
    let referrers = r.block_referrers(TARGET).expect("list");
    let mut t = titles(&referrers);
    t.sort();
    assert_eq!(
        t,
        [
            format!("see (({TARGET}))"),
            format!("{{{{embed (({TARGET}))}}}}")
        ]
    );
    let resolved = r.resolve_block_ref(TARGET).expect("resolve").expect("some");
    assert_eq!(resolved.title, "the target");
    // Dangling: referenced but missing.
    assert_eq!(r.block_ref_count(DANGLING).expect("count"), 1);
    assert!(r.resolve_block_ref(DANGLING).expect("resolve").is_none());
    assert_eq!(
        r.block_ref_count("6650a1b2-0000-4000-8000-0000000000ff")
            .expect("count"),
        0
    );
}

fn tasks_graph() -> read_common::Fixture {
    indexed(&[
        (
            "pages/Work.md",
            "- TODO [#A] write report\n  SCHEDULED: <2026-10-06 Tue>\n\
             - DOING review\n  DEADLINE: <2026-10-08 Thu>\n\
             - DONE shipped\n  SCHEDULED: <2026-10-06 Tue>\n\
             - CANCELED dropped\n  DEADLINE: <2026-10-07 Wed>\n\
             - LATER someday\n\
             - TODO weekly\n  SCHEDULED: <2026-09-01 Tue .+1w>\n\
             - TODO far away\n  SCHEDULED: <2026-12-01 Tue>\n\
             - TODO past\n  SCHEDULED: <2026-09-30 Wed>\n",
        ),
        ("pages/Home.md", "- TODO [#B] water plants\n"),
    ])
}

#[test]
fn tasks_filter_by_marker_priority_and_page() {
    let fx = tasks_graph();
    let r = &fx.reader;
    let all = r.tasks(&TaskFilter::default()).expect("tasks");
    assert_eq!(all.len(), 9);
    let todo = r
        .tasks(&TaskFilter {
            markers: vec!["TODO".into()],
            ..TaskFilter::default()
        })
        .expect("tasks");
    assert_eq!(todo.len(), 5);
    let high = r
        .tasks(&TaskFilter {
            priority: Some("A".into()),
            ..TaskFilter::default()
        })
        .expect("tasks");
    assert_eq!(
        titles(&high.iter().map(|t| t.block.clone()).collect::<Vec<_>>()),
        ["write report"]
    );
    assert_eq!(high[0].page_name, "Work");
    let home = r
        .tasks(&TaskFilter {
            page_id: Some(fx.page_id("home")),
            ..TaskFilter::default()
        })
        .expect("tasks");
    assert_eq!(home.len(), 1);
}

#[test]
fn agenda_window_excludes_done_and_includes_repeating() {
    let fx = tasks_graph();
    let items = fx.reader.agenda(20_261_006, 3).expect("agenda");
    let got: Vec<(String, AgendaKind, i64)> = items
        .iter()
        .map(|i| (i.task.block.title.clone(), i.kind, i.day))
        .collect();
    assert_eq!(
        got,
        [
            ("weekly".to_owned(), AgendaKind::Repeated, 20_260_901),
            ("write report".to_owned(), AgendaKind::Scheduled, 20_261_006),
            ("review".to_owned(), AgendaKind::Deadline, 20_261_008),
        ]
    );
    // DONE / CANCELED / out-of-window items are absent; a wider window pulls in later ones.
    let wide = fx.reader.agenda(20_261_006, 90).expect("agenda");
    assert!(wide.iter().any(|i| i.task.block.title == "far away"));
    assert!(
        wide.iter()
            .all(|i| i.task.block.marker.as_deref() != Some("DONE"))
    );
}

#[test]
fn namespace_children_and_tree_depth_three() {
    let fx = indexed(&[
        ("pages/plan.md", "title:: work/q3/plan\n\n- plan\n"),
        ("pages/q3.md", "title:: work/q3\n\n- q3\n"),
        ("pages/q4.md", "title:: work/q4\n\n- q4\n"),
        ("pages/work.md", "- root\n"),
        ("pages/unrelated.md", "- x\n"),
    ]);
    let r = &fx.reader;
    let work = fx.page_id("work");
    let kids: Vec<_> = r
        .namespace_children(work)
        .expect("children")
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(kids, ["work/q3", "work/q4"]);
    let tree: Vec<(String, usize)> = r
        .namespace_tree(work)
        .expect("tree")
        .into_iter()
        .map(|n| (n.page.name, n.depth))
        .collect();
    assert_eq!(
        tree,
        [
            ("work".to_owned(), 0),
            ("work/q3".to_owned(), 1),
            ("work/q3/plan".to_owned(), 2),
            ("work/q4".to_owned(), 1),
        ]
    );
}

#[test]
fn graph_edges_honour_options_and_exclusions() {
    let fx = indexed(&[
        ("pages/Alpha.md", "- links [[Beta]] and [[ns/child]]\n"),
        ("pages/Beta.md", "- back [[Alpha]]\n"),
        ("pages/Lonely.md", "- nothing\n"),
        (
            "pages/Hidden.md",
            "exclude-from-graph-view:: true\n\n- secret [[Alpha]]\n",
        ),
        (
            "journals/2026_10_06.md",
            "- today [[Alpha]]\n- done TODO later\n",
        ),
    ]);
    let r = &fx.reader;
    let id = |n: &str| fx.page_id(n);
    let default = r.graph_edges(GraphOptions::default()).expect("graph");
    let names: Vec<&str> = default.nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(
        names.contains(&"Lonely"),
        "orphans on by default: {names:?}"
    );
    assert!(!names.contains(&"Hidden"), "{names:?}");
    assert!(default.nodes.iter().all(|n| !n.is_journal && !n.is_builtin));
    let has = |kind, a: &str, b: &str| {
        default
            .edges
            .iter()
            .any(|e| e.kind == kind && e.src == id(a) && e.dst == id(b))
    };
    assert!(has(GraphEdgeKind::Ref, "alpha", "beta"));
    assert!(has(GraphEdgeKind::Ref, "beta", "alpha"));
    assert!(has(GraphEdgeKind::Ref, "alpha", "ns/child"));
    assert!(has(GraphEdgeKind::Namespace, "ns/child", "ns"));
    assert!(
        !default
            .edges
            .iter()
            .any(|e| e.src == id("hidden") || e.dst == id("hidden"))
    );

    let no_orphans = r
        .graph_edges(GraphOptions {
            orphans: false,
            ..GraphOptions::default()
        })
        .expect("graph");
    assert!(no_orphans.nodes.iter().all(|n| n.name != "Lonely"));

    let with_journals = r
        .graph_edges(GraphOptions {
            journals: true,
            builtins: true,
            ..GraphOptions::default()
        })
        .expect("graph");
    assert!(with_journals.nodes.iter().any(|n| n.is_journal));
    assert!(with_journals.nodes.iter().any(|n| n.is_builtin));
    assert!(with_journals.edges.len() > default.edges.len());
}

#[test]
fn backlink_counts_search_and_mentions_back_the_app_views() {
    let fx = indexed(&[
        ("pages/Alpha.md", "- see [[Beta]]\n- again [[Beta]]\n"),
        ("pages/Beta.md", "- ![x](../assets/x.png)\n"),
        ("pages/Gamma.md", "- unrelated\n"),
    ]);
    let r = &fx.reader;
    let beta = r.page_id("Beta").expect("id").expect("page");
    let counts = r.backlink_counts().expect("counts");
    assert_eq!(counts.get(&beta), Some(&2));
    let gamma = r.page_id("Gamma").expect("id").expect("page");
    assert_eq!(counts.get(&gamma), None);
    let hits = r
        .search("beta", &bitacora_index::search::SearchOptions::default())
        .expect("search");
    assert!(matches!(
        hits.first(),
        Some(bitacora_index::search::SearchHit::Page { title, .. }) if title == "Beta"
    ));
    let mentions = r.blocks_mentioning("assets/x.png", 10).expect("mentions");
    assert_eq!(mentions.len(), 1);
    assert!(r.blocks_mentioning("", 10).expect("empty").is_empty());
    assert!(
        r.blocks_mentioning("nope.png", 10)
            .expect("none")
            .is_empty()
    );
}

#[test]
fn templates_are_listed_by_name_with_their_page() {
    let fx = indexed(&[
        (
            "pages/Tpl.md",
            "- Meeting\n  template:: meeting\n  - child\n- Standup\n  template:: Daily\n",
        ),
        ("pages/Other.md", "- plain\n"),
    ]);
    assert_eq!(
        fx.reader.templates().expect("templates"),
        vec![
            ("Daily".to_owned(), "Tpl".to_owned()),
            ("meeting".to_owned(), "Tpl".to_owned())
        ]
    );
}
