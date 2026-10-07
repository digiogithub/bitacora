//! Graph-view data: nodes, edges, degree, filters, local graph (BIT-SP-0012.R1/R2; BIT-T-0470/0471).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod read_common;

use bitacora_config::EffectiveConfig;
use bitacora_index::{GraphData, GraphFilter, Indexer, IndexerOptions};
use read_common::{common, indexed};

const UUID_NAME: &str = "6650a1b2-0000-4000-8000-000000000001";

fn files() -> Vec<(&'static str, String)> {
    vec![
        (
            "pages/Alpha.md",
            format!(
                "tags:: topic\n\n- links [[Beta]] and [[ns/child]] and #hot\n- self [[Alpha]]\n\
                 - dup [[Beta]]\n- img [[assets/pic.png]]\n\
                 - uuid [[{UUID_NAME}]]\n"
            ),
        ),
        ("pages/Beta.md", "- back [[Alpha]]\n".into()),
        ("pages/Lonely.md", "- nothing\n".into()),
        (
            "pages/Hidden.md",
            "exclude-from-graph-view:: true\n\n- secret [[Alpha]]\n".into(),
        ),
        (
            "journals/2026_10_06.md",
            "- today [[Alpha]]\n- TODO later\n".into(),
        ),
    ]
}

fn indexed_files(f: &[(&str, String)]) -> read_common::Fixture {
    let v: Vec<(&str, &str)> = f.iter().map(|(p, t)| (*p, t.as_str())).collect();
    indexed(&v)
}

type Canon = (Vec<(String, u32, bool, bool, bool)>, Vec<(String, String)>);

/// Id-independent view: node names + degree + flags, and named edges.
fn canon(d: &GraphData) -> Canon {
    let name = |id: i64| {
        d.nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.name.clone())
            .expect("edge endpoint is a node")
    };
    let mut nodes: Vec<_> = d
        .nodes
        .iter()
        .map(|n| {
            (
                n.name.clone(),
                n.degree,
                n.is_journal,
                n.is_tag,
                n.is_namespace_parent,
            )
        })
        .collect();
    nodes.sort();
    let mut edges: Vec<_> = d.edges.iter().map(|e| (name(e.src), name(e.dst))).collect();
    edges.sort();
    (nodes, edges)
}

#[test]
fn global_graph_nodes_edges_degree_and_exclusions() {
    let fx = indexed_files(&files());
    let d = fx
        .reader
        .graph_data(&GraphFilter::default())
        .expect("graph");
    let (nodes, edges) = canon(&d);
    let names: Vec<&str> = nodes.iter().map(|n| n.0.as_str()).collect();
    assert!(
        names.contains(&"Lonely"),
        "orphans on by default: {names:?}"
    );
    assert!(
        names.contains(&"hot"),
        "placeholder page included: {names:?}"
    );
    assert!(!names.contains(&"Hidden"), "{names:?}");
    assert!(!names.iter().any(|n| n.contains("assets/")), "{names:?}");
    assert!(!names.contains(&UUID_NAME), "{names:?}");
    assert!(nodes.iter().all(|n| !n.2), "journals hidden by default");
    assert!(edges.iter().all(|(a, b)| a != b), "no self links");
    let count = |a: &str, b: &str| edges.iter().filter(|e| e.0 == a && e.1 == b).count();
    assert_eq!(count("Alpha", "Beta"), 1);
    assert_eq!(count("Beta", "Alpha"), 1);
    assert_eq!(count("Alpha", "topic"), 1, "tags:: edge");
    assert_eq!(count("Alpha", "hot"), 1, "#tag edge");
    assert_eq!(count("ns/child", "ns"), 1, "namespace edge");
    let get = |n: &str| nodes.iter().find(|x| x.0 == n).expect("node").clone();
    assert!(get("hot").3 && get("topic").3, "tag flags");
    assert!(get("ns").4, "namespace parent flag");
    assert_eq!(get("Lonely").1, 0);
    // Alpha: ->Beta, <-Beta, ->topic, ->hot, ->ns/child
    assert_eq!(get("Alpha").1, 5);
}

#[test]
fn filters_journals_orphans_and_excluded_pages() {
    let fx = indexed_files(&files());
    let r = &fx.reader;
    let with_j = r
        .graph_data(&GraphFilter {
            journals: true,
            ..GraphFilter::default()
        })
        .expect("graph");
    let (nodes, edges) = canon(&with_j);
    assert!(nodes.iter().any(|n| n.2), "journal visible");
    assert!(edges.iter().any(|e| e.1 == "Alpha" && e.0.contains("2026")));
    let no_orphans = r
        .graph_data(&GraphFilter {
            orphans: false,
            ..GraphFilter::default()
        })
        .expect("graph");
    assert!(no_orphans.nodes.iter().all(|n| n.degree > 0));
    assert!(no_orphans.nodes.iter().all(|n| n.name != "Lonely"));
    let ex = r
        .graph_data(&GraphFilter {
            excluded_pages: vec!["BETA".into()],
            ..GraphFilter::default()
        })
        .expect("graph");
    assert!(ex.nodes.iter().all(|n| n.name != "Beta"));
    let alpha = ex.nodes.iter().find(|n| n.name == "Alpha").expect("alpha");
    assert_eq!(alpha.degree, 3, "edges to excluded pages are dropped");
}

#[test]
fn local_graph_is_one_hop_with_neighbour_links() {
    let fx = indexed_files(&[
        ("pages/Centre.md", "- [[Aaa]] [[Bbb]]\n".into()),
        ("pages/Aaa.md", "- [[Bbb]] [[Far]]\n".into()),
        ("pages/Bbb.md", "- x\n".into()),
        ("pages/Far.md", "- y\n".into()),
        ("pages/Back.md", "- [[Centre]]\n".into()),
        ("journals/2026_10_06.md", "- [[Centre]]\n".into()),
    ]);
    let r = &fx.reader;
    let d = r
        .local_graph_data(fx.page_id("centre"), &GraphFilter::default())
        .expect("local");
    let (nodes, edges) = canon(&d);
    let names: Vec<&str> = nodes.iter().map(|n| n.0.as_str()).collect();
    assert_eq!(
        names,
        ["Aaa", "Back", "Bbb", "Centre"],
        "journals off, no 2-hop"
    );
    assert!(
        edges.contains(&("Aaa".into(), "Bbb".into())),
        "neighbour link"
    );
    assert_eq!(edges.len(), 4);
    let d = r
        .local_graph_data(
            fx.page_id("centre"),
            &GraphFilter {
                journals: true,
                ..GraphFilter::default()
            },
        )
        .expect("local");
    assert_eq!(d.nodes.len(), 5);
    // The centre stays even when it would be filtered out (a journal with journals off).
    let jid = d
        .nodes
        .iter()
        .find(|n| n.is_journal)
        .map(|n| n.id)
        .expect("journal node");
    let d = r
        .local_graph_data(jid, &GraphFilter::default())
        .expect("local journal");
    assert!(d.nodes.iter().any(|n| n.id == jid));
}

#[test]
fn incremental_equals_rebuild() {
    let mut v2 = files();
    v2[1].1 = "- back [[Alpha]] and [[Gamma]]\n".into();
    v2.push(("pages/Gamma.md", "- [[Lonely]]\n".into()));
    let fresh = indexed_files(&v2);

    let inc = indexed_files(&files());
    for (p, t) in &v2 {
        common::write(&inc.env.graph, p, t);
    }
    let ix = Indexer::start(
        &inc.index,
        IndexerOptions::new(&inc.env.graph, EffectiveConfig::default()),
    )
    .expect("indexer");
    ix.reconcile().expect("reconcile");
    ix.shutdown();

    let f = GraphFilter {
        journals: true,
        ..GraphFilter::default()
    };
    let a = canon(&fresh.reader.graph_data(&f).expect("a"));
    let b = canon(&inc.reader.graph_data(&f).expect("b"));
    assert_eq!(a, b);
    assert!(a.1.contains(&("Beta".into(), "Gamma".into())));
}
