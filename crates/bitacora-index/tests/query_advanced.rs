//! Advanced (Datalog subset) query corpus (BIT-US-0103, BIT-SP-0003.R19). Every file of
//! `fixtures/queries/advanced/` is a query written for this project, classified as supported,
//! supported-with-warning or unsupported/error, with the expected rows for the supported ones.
//! Expected rows follow the documented Logseq semantics; the black-box comparison against the
//! Logseq desktop app is pending (needs the Logseq runtime).
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod common;

use std::path::PathBuf;

use bitacora_core::date::Date;
use bitacora_index::query::advanced::{Cell, Shape};
use bitacora_index::query::{QueryContext, QueryError};
use common::{env, input, writer};

const FILES: &[(&str, &str)] = &[
    (
        "pages/Alpha.md",
        "type:: book\ntags:: fiction, classic\nrating:: 5\n\n\
         - TODO [#A] read chapter one #reading\n  status:: active\n  rating:: 4\n  - child of reading task\n\
         - DONE finished chapter zero [[Beta]]\n\
         - DOING [#B] skim notes\n  status:: paused\n  rating:: 9\n\
         - plain note about standup\n",
    ),
    (
        "pages/Beta.md",
        "type:: person\n\n\
         - TODO call [[Alpha]] about it\n\
         - LATER do someday\n\
         - note with property\n  published:: true\n  count:: 3\n  created-at:: 1760000000000\n",
    ),
    (
        "pages/Proj___Sub1.md",
        "title:: Proj/Sub1\n\n- TODO sub task\n",
    ),
    ("pages/Proj___Sub2.md", "title:: Proj/Sub2\n\n- sub two\n"),
    (
        "pages/Proj___Sub1___Deep.md",
        "title:: Proj/Sub1/Deep\n\n- deep\n",
    ),
    (
        "journals/2026_10_01.md",
        "- TODO standup notes\n- DONE old chore\n",
    ),
    (
        "journals/2026_10_05.md",
        "- standup today\n- TODO file report [[Alpha]]\n",
    ),
    (
        "journals/2026_10_06.md",
        "- today entry about standup\n- NOW fix build\n",
    ),
];

fn ctx() -> QueryContext {
    QueryContext::new(Date::new(2026, 10, 6).expect("date"), 1_791_000_000_000)
}

fn label(c: &Cell) -> String {
    match c {
        Cell::Null => "nil".to_owned(),
        Cell::Int(n) => n.to_string(),
        Cell::Real(x) => x.to_string(),
        Cell::Text(s) => s.clone(),
        Cell::Block(b) if b.is_pre_block => "<pre>".to_owned(),
        Cell::Block(b) => b.title.clone(),
        Cell::Page(p) => p.name.clone(),
    }
}

struct Case {
    name: String,
    class: String,
    expect: String,
    error: String,
    query: String,
}

fn corpus() -> Vec<Case> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/queries/advanced");
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).expect("corpus dir") {
        let path = e.expect("entry").path();
        if path.extension().and_then(|x| x.to_str()) != Some("edn") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read");
        let mut c = Case {
            name: path.file_stem().unwrap().to_string_lossy().into_owned(),
            class: String::new(),
            expect: String::new(),
            error: String::new(),
            query: String::new(),
        };
        for line in text.lines() {
            if let Some(v) = line.strip_prefix(";; class: ") {
                c.class = v.trim().to_owned();
            } else if let Some(v) = line.strip_prefix(";; expect: ") {
                c.expect = v.trim().to_owned();
            } else if let Some(v) = line.strip_prefix(";; error: ") {
                c.error = v.trim().to_owned();
            } else {
                c.query.push_str(line);
                c.query.push('\n');
            }
        }
        out.push(c);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn expected_rows(s: &str) -> Vec<String> {
    let mut v: Vec<String> = s.split(" | ").map(str::to_owned).collect();
    v.sort();
    v
}

#[test]
fn corpus_is_classified_and_supported_queries_return_the_expected_rows() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    for (p, t) in FILES {
        w.replace_file(input(p, t)).expect("replace");
    }
    let r = index.read_api();
    let cases = corpus();
    assert!(cases.len() >= 25, "corpus too small: {}", cases.len());

    let (mut supported, mut warned, mut rejected) = (0, 0, 0);
    for c in &cases {
        let res = r.query_advanced(&c.query, &ctx());
        match c.class.as_str() {
            "supported" | "warning:result-transform" | "warning:view" | "warning:pull pattern" => {
                let out = res.unwrap_or_else(|e| panic!("{}: {e}\n{}", c.name, c.query));
                let mut got: Vec<String> = out
                    .rows
                    .iter()
                    .map(|row| row.iter().map(label).collect::<Vec<_>>().join(","))
                    .collect();
                got.sort();
                assert_eq!(
                    got,
                    expected_rows(&c.expect),
                    "{}\nSQL: {}",
                    c.name,
                    out.sql.clone().unwrap_or_default()
                );
                if let Some(w) = c.class.strip_prefix("warning:") {
                    warned += 1;
                    assert!(
                        out.warnings.iter().any(|x| x.construct.contains(w)),
                        "{}: {:?}",
                        c.name,
                        out.warnings
                    );
                    assert!(
                        out.warnings[0].to_string().starts_with("unsupported: "),
                        "{}",
                        c.name
                    );
                } else {
                    supported += 1;
                    assert!(out.warnings.is_empty(), "{}: {:?}", c.name, out.warnings);
                }
            }
            "unsupported" => {
                rejected += 1;
                match res {
                    Err(QueryError::Unsupported(m)) => {
                        assert!(m.contains(&c.error), "{}: {m}", c.name);
                    }
                    other => panic!("{}: expected unsupported, got {other:?}", c.name),
                }
            }
            "error" => {
                rejected += 1;
                match res {
                    Err(QueryError::Syntax(m)) => assert!(m.contains(&c.error), "{}: {m}", c.name),
                    other => panic!("{}: expected a syntax error, got {other:?}", c.name),
                }
            }
            other => panic!("{}: unknown class {other}", c.name),
        }
    }
    println!(
        "advanced corpus: {} queries, {} supported, {} supported with warnings, {} rejected \
         ({:.0}% evaluated)",
        cases.len(),
        supported,
        warned,
        rejected,
        100.0 * f64::from(supported + warned) / cases.len() as f64
    );
}

#[test]
fn shapes_titles_and_metadata() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    for (p, t) in FILES {
        w.replace_file(input(p, t)).expect("replace");
    }
    let r = index.read_api();
    let out = r
        .query_advanced(
            "#+BEGIN_QUERY\n{:title \"Open\" :collapsed? true\n :query [:find (count ?b) . :where [?b :block/marker \"TODO\"]]}\n#+END_QUERY",
            &ctx(),
        )
        .expect("run");
    assert_eq!(out.title.as_deref(), Some("Open"));
    assert!(out.collapsed);
    assert_eq!(out.shape, Shape::Scalar);
    assert_eq!(out.rows, vec![vec![Cell::Int(5)]]);

    let out = r
        .query_advanced("[:find [?m ...] :where [?b :block/marker ?m]]", &ctx())
        .expect("run");
    assert_eq!(out.shape, Shape::Collection);
    assert_eq!(out.rows.len(), 5);
    assert_eq!(out.columns, ["?m"]);
    // A map without :title and a bare string query work as well.
    let out = r
        .query_advanced("{:query \"standup\"}", &ctx())
        .expect("run");
    assert_eq!(out.blocks().len(), 4);
    // Page inputs: `"[[Alpha]]"` is lower-cased into the page name.
    let out = r
        .query_advanced(
            "{:query [:find (pull ?b [*]) :in $ ?name :where [?b :block/page ?p] [?p :block/name ?name] [?b :block/marker \"DOING\"]] :inputs [\"[[Alpha]]\"]}",
            &ctx(),
        )
        .expect("run");
    assert_eq!(out.blocks().len(), 1);
    // :current-page
    let mut c = ctx();
    c.current_page = Some("alpha".into());
    let out = r
        .query_advanced(
            "{:query [:find (pull ?b [*]) :in $ ?name :where [?b :block/page ?p] [?p :block/name ?name] [?b :block/marker \"DOING\"]] :inputs [:current-page]}",
            &c,
        )
        .expect("run");
    assert_eq!(out.blocks().len(), 1);
    // Input count mismatch and a missing :current-page are syntax errors.
    assert!(matches!(
        r.query_advanced(
            "{:query [:find ?b :in $ ?x :where [?b :block/marker ?x]] :inputs []}",
            &ctx()
        ),
        Err(QueryError::Syntax(_))
    ));
    assert!(matches!(
        r.query_advanced(
            "{:query [:find ?b :in $ ?x :where [?b :block/marker ?x]] :inputs [:current-page]}",
            &ctx()
        ),
        Err(QueryError::Syntax(_))
    ));
}

#[test]
fn hostile_constants_are_parameters() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    for (p, t) in FILES {
        w.replace_file(input(p, t)).expect("replace");
    }
    let r = index.read_api();
    let evil = "x' OR 1=1; DROP TABLE blocks; --";
    let q = format!(
        "[:find (pull ?b [*]) :where [?b :block/content ?c] [(clojure.string/includes? ?c \"{}\")] [?b :block/marker \"{}\"]]",
        evil.replace('"', "\\\""),
        evil
    );
    let out = r.query_advanced(&q, &ctx()).expect("run");
    assert!(out.rows.is_empty());
    let sql = out.sql.expect("sql");
    assert!(!sql.contains("DROP") && !sql.contains("1=1"), "{sql}");
    // An invalid regular expression is rejected before it reaches SQLite.
    assert!(matches!(
        r.query_advanced(
            "[:find ?b :where [?b :block/content ?c] [(re-find #\"(\" ?c)]]",
            &ctx()
        ),
        Err(QueryError::Syntax(_))
    ));
    assert_eq!(
        r.query_simple("(task TODO)", &ctx())
            .expect("still there")
            .blocks
            .len(),
        5
    );
}
