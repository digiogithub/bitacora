//! Simple query DSL conformance suite (BIT-US-0101, BIT-SP-0003.R18): every operator and
//! combinations over a fixed graph, with `today = 2026-10-06`. Expected results are derived from
//! the documented Logseq semantics (design §7, analysis 03 §8.1); a black-box comparison against
//! the Logseq desktop app is pending (needs the Logseq runtime).
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod common;

use bitacora_core::date::Date;
use bitacora_index::query::{QueryContext, QueryError, ResultKind, compile_simple};
use bitacora_index::{BlockRow, Index, IndexReader, IndexWriter};
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

struct Fx {
    _env: common::Env,
    _index: Index,
    _w: IndexWriter,
    r: IndexReader,
}

fn fx() -> Fx {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    for (p, t) in FILES {
        w.replace_file(input(p, t)).expect("replace");
    }
    let r = index.read_api();
    Fx {
        _env: env,
        _index: index,
        _w: w,
        r,
    }
}

fn ctx() -> QueryContext {
    QueryContext::new(Date::new(2026, 10, 6).expect("date"), 1_791_000_000_000)
}

fn label(b: &BlockRow) -> String {
    if b.is_pre_block {
        "<pre>".to_owned()
    } else {
        b.title.clone()
    }
}

impl Fx {
    /// Block titles in result order.
    fn ordered(&self, q: &str) -> Vec<String> {
        let res = self
            .r
            .query_simple(q, &ctx())
            .unwrap_or_else(|e| panic!("{q}: {e}"));
        assert_eq!(res.kind, ResultKind::Blocks, "{q}");
        res.blocks.iter().map(label).collect()
    }

    fn blocks(&self, q: &str) -> Vec<String> {
        let mut v = self.ordered(q);
        v.sort();
        v
    }

    fn pages(&self, q: &str) -> Vec<String> {
        let res = self
            .r
            .query_simple(q, &ctx())
            .unwrap_or_else(|e| panic!("{q}: {e}"));
        assert_eq!(res.kind, ResultKind::Pages, "{q}");
        let mut v: Vec<String> = res.pages.iter().map(|p| p.name.clone()).collect();
        v.sort();
        v
    }
}

fn s(items: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = items.iter().map(|x| (*x).to_owned()).collect();
    v.sort();
    v
}

#[test]
fn task_markers() {
    let f = fx();
    assert_eq!(
        f.blocks("(task TODO)"),
        s(&[
            "call [[Alpha]] about it",
            "file report [[Alpha]]",
            "read chapter one #reading",
            "standup notes",
            "sub task"
        ])
    );
    assert_eq!(
        f.blocks("(todo done)"),
        s(&["finished chapter zero [[Beta]]", "old chore"])
    );
    assert_eq!(
        f.blocks("(task [NOW LATER])"),
        s(&["do someday", "fix build"])
    );
    assert_eq!(f.blocks("(task Doing)"), s(&["skim notes"]));
    assert_eq!(f.blocks("(task NOPE)"), s(&[]));
}

#[test]
fn priority() {
    let f = fx();
    assert_eq!(f.blocks("(priority a)"), s(&["read chapter one #reading"]));
    assert_eq!(
        f.blocks("(priority A B)"),
        s(&["read chapter one #reading", "skim notes"])
    );
    assert_eq!(
        f.blocks("(and (task TODO) (priority a))"),
        s(&["read chapter one #reading"])
    );
    assert_eq!(f.blocks("(priority c)"), s(&[]));
}

#[test]
fn boolean_operators_and_null_safe_not() {
    let f = fx();
    assert_eq!(
        f.blocks("(and (task TODO DOING) (not (priority a)))"),
        s(&[
            "call [[Alpha]] about it",
            "file report [[Alpha]]",
            "skim notes",
            "standup notes",
            "sub task"
        ])
    );
    // DONE blocks have no priority: NOT must keep them (NULL-safe negation).
    assert_eq!(
        f.blocks("(and (task DONE) (not (priority A)))"),
        s(&["finished chapter zero [[Beta]]", "old chore"])
    );
    assert_eq!(
        f.blocks("(or (priority A) (task LATER))"),
        s(&["read chapter one #reading", "do someday"])
    );
    // `not` with several clauses negates their conjunction.
    assert_eq!(
        f.blocks("(and (task TODO DONE) (not (task DONE) (priority a)))"),
        s(&[
            "call [[Alpha]] about it",
            "file report [[Alpha]]",
            "finished chapter zero [[Beta]]",
            "old chore",
            "read chapter one #reading",
            "standup notes",
            "sub task"
        ])
    );
    // Nested and/or/not.
    assert_eq!(
        f.blocks("(and (or (task TODO) (task DONE)) (not (or (task DONE) (page alpha))))"),
        s(&[
            "call [[Alpha]] about it",
            "file report [[Alpha]]",
            "standup notes",
            "sub task"
        ])
    );
}

#[test]
fn page_refs_use_path_refs() {
    let f = fx();
    // Blocks on page Alpha plus blocks referencing it (and their descendants).
    assert_eq!(
        f.blocks("(and [[Alpha]] (task TODO))"),
        s(&[
            "call [[Alpha]] about it",
            "file report [[Alpha]]",
            "read chapter one #reading"
        ])
    );
    assert_eq!(f.blocks("(and [[Beta]] (task LATER))"), s(&["do someday"]));
    // A descendant of a referencing block matches too.
    assert_eq!(
        f.blocks("(and [[reading]] (not (task TODO)))"),
        s(&["child of reading task"])
    );
    assert_eq!(f.blocks("#reading"), f.blocks("[[reading]]"));
    assert_eq!(f.blocks("#[[reading]]"), f.blocks("[[Reading]]"));
    assert_eq!(f.blocks("[[reading]]").len(), 2);
    assert_eq!(f.blocks("[[does not exist]]"), s(&[]));
    // Implicit and of several top-level forms.
    assert_eq!(f.blocks("[[Beta]] (task LATER)"), s(&["do someday"]));
    // Page references via property values are references too.
    assert!(f.blocks("[[fiction]]").contains(&"<pre>".to_owned()));
}

#[test]
fn full_text() {
    let f = fx();
    let standup = s(&[
        "plain note about standup",
        "standup notes",
        "standup today",
        "today entry about standup",
    ]);
    assert_eq!(f.blocks("\"standup\""), standup);
    assert_eq!(f.blocks("(and \"STANDUP\")"), standup);
    assert_eq!(f.blocks("{{query \"standup\"}}"), standup);
    // Substring, not word match.
    assert_eq!(f.blocks("\"tandu\"").len(), 4);
    // Short strings fall back to instr().
    assert_eq!(f.blocks("(and \"up\" (task TODO))"), s(&["standup notes"]));
    // FTS syntax inside the string is neutralised.
    assert_eq!(f.blocks("\"standup\\\" OR \\\"chapter\""), s(&[]));
    assert_eq!(f.blocks("\"a* NEAR b\""), s(&[]));
}

#[test]
fn block_properties() {
    let f = fx();
    assert_eq!(
        f.blocks("(property status active)"),
        s(&["read chapter one #reading"])
    );
    assert_eq!(
        f.blocks("(property STATUS Active)"),
        s(&["read chapter one #reading"])
    );
    assert_eq!(
        f.blocks("(property status)"),
        s(&["read chapter one #reading", "skim notes"])
    );
    assert_eq!(
        f.blocks("(property rating 4)"),
        s(&["read chapter one #reading"])
    );
    assert_eq!(f.blocks("(property rating 9)"), s(&["skim notes"]));
    assert_eq!(f.blocks("(property rating 5)"), s(&["<pre>"]));
    assert_eq!(
        f.blocks("(property published true)"),
        s(&["note with property"])
    );
    assert_eq!(f.blocks("(property published false)"), s(&[]));
    assert_eq!(f.blocks("(property count 3)"), s(&["note with property"]));
    // Sets: tags contains the value; `#x` and `[[x]]` forms are decoded.
    assert_eq!(f.blocks("(property tags fiction)"), s(&["<pre>"]));
    assert_eq!(f.blocks("(property tags #classic)"), s(&["<pre>"]));
    assert_eq!(f.blocks("(property tags [[Classic]])"), s(&["<pre>"]));
    assert_eq!(f.blocks("(property type book)"), s(&["<pre>"]));
    assert_eq!(f.blocks("(property tags nothing)"), s(&[]));
    assert_eq!(f.blocks("(property no-such-key)"), s(&[]));
}

#[test]
fn page_properties_tags_and_namespace() {
    let f = fx();
    assert_eq!(f.pages("(page-property type book)"), s(&["alpha"]));
    assert_eq!(f.pages("(page-property type [[Person]])"), s(&["beta"]));
    assert_eq!(f.pages("(page-property type)"), s(&["alpha", "beta"]));
    assert_eq!(f.pages("(page-property rating 5)"), s(&["alpha"]));
    assert_eq!(f.pages("(page-tags fiction)"), s(&["alpha"]));
    assert_eq!(f.pages("(page-tags [fiction nothing])"), s(&["alpha"]));
    assert_eq!(f.pages("(page-tags classic fiction)"), s(&["alpha"]));
    assert_eq!(f.pages("(page-tags nothing)"), s(&[]));
    assert_eq!(
        f.pages("(and (page-property type book) (page-tags fiction))"),
        s(&["alpha"])
    );
    assert_eq!(
        f.pages("(or (page-property type book) (page-property type person))"),
        s(&["alpha", "beta"])
    );
    let not_book = f.pages("(not (page-property type book))");
    assert!(!not_book.contains(&"alpha".to_owned()));
    assert!(not_book.contains(&"proj/sub1".to_owned()));
    assert_eq!(f.pages("(all-page-tags)"), s(&["classic", "fiction"]));
    // Direct children only.
    assert_eq!(f.pages("(namespace proj)"), s(&["proj/sub1", "proj/sub2"]));
    assert_eq!(f.pages("(namespace [[Proj/Sub1]])"), s(&["proj/sub1/deep"]));
    assert_eq!(f.pages("(namespace nothing)"), s(&[]));
}

#[test]
fn page_level_filters_inside_block_queries() {
    let f = fx();
    assert_eq!(
        f.blocks("(and (task TODO) (page-tags fiction))"),
        s(&["read chapter one #reading"])
    );
    assert_eq!(
        f.blocks("(and (task TODO) (namespace proj))"),
        s(&["sub task"])
    );
    assert_eq!(
        f.blocks("(and (task TODO) (page-property type person))"),
        s(&["call [[Alpha]] about it"])
    );
    assert_eq!(
        f.blocks("(and (task TODO) (not (page-property type)))"),
        s(&["file report [[Alpha]]", "standup notes", "sub task"])
    );
}

#[test]
fn page_filter() {
    let f = fx();
    assert_eq!(
        f.blocks("(and (page alpha) (task TODO))"),
        s(&["read chapter one #reading"])
    );
    assert_eq!(
        f.blocks("(and (page [[Alpha]]) (task DOING))"),
        s(&["skim notes"])
    );
    assert_eq!(
        f.blocks("(and (page \"Alpha\") (priority a))"),
        s(&["read chapter one #reading"])
    );
    assert_eq!(f.blocks("(page nothing)"), s(&[]));
    assert_eq!(
        f.blocks("(and (page alpha) (not (task TODO DONE DOING)))")
            .len(),
        3
    );
}

#[test]
fn between_journal_days() {
    let f = fx();
    assert_eq!(
        f.blocks("(and (between -2d today) (task TODO NOW))"),
        s(&["file report [[Alpha]]", "fix build"])
    );
    assert_eq!(
        f.blocks("(and (between today -2d) (task TODO NOW))"),
        s(&["file report [[Alpha]]", "fix build"])
    );
    assert_eq!(
        f.blocks("(and (between [[Oct 5th, 2026]] [[Oct 6th, 2026]]) (task TODO NOW))"),
        s(&["file report [[Alpha]]", "fix build"])
    );
    assert_eq!(
        f.blocks("(and (between yesterday tomorrow) (task TODO NOW))"),
        s(&["file report [[Alpha]]", "fix build"])
    );
    assert_eq!(
        f.blocks("(and (between today tomorrow) (task TODO))"),
        s(&[])
    );
    assert_eq!(
        f.blocks("(and (between -1w +1d) (task TODO))"),
        s(&["file report [[Alpha]]", "standup notes"])
    );
    assert_eq!(f.blocks("(between -1m -1d)").len(), 4);
    // The design example: journal window, negated task, text.
    assert_eq!(
        f.blocks("(and (between -7d today) (not (task DONE)) \"standup\")"),
        s(&[
            "standup notes",
            "standup today",
            "today entry about standup"
        ])
    );
    // Non-journal pages are never in a `between` window.
    assert!(
        !f.blocks("(between -3650d +3650d)")
            .contains(&"read chapter one #reading".to_owned())
    );
}

#[test]
fn between_timestamps() {
    let f = fx();
    assert_eq!(
        f.blocks("(between created-at [[Oct 9th, 2025]] [[Oct 10th, 2025]])"),
        s(&["note with property"])
    );
    assert_eq!(
        f.blocks("(between created_at [[Oct 10th, 2025]] [[Oct 9th, 2025]])"),
        s(&["note with property"])
    );
    assert_eq!(f.blocks("(between created-at -1d now)"), s(&[]));
    assert_eq!(f.blocks("(between created-at -30w -1d)").len(), 0);
    assert_eq!(
        f.blocks("(between created-at -2y now)"),
        s(&["note with property"])
    );
}

#[test]
fn sort_and_sample() {
    let f = fx();
    assert_eq!(
        f.ordered("(and (property rating) (sort-by rating asc))"),
        ["read chapter one #reading", "<pre>", "skim notes"]
    );
    assert_eq!(
        f.ordered("(and (property rating) (sort-by rating))"),
        ["skim notes", "<pre>", "read chapter one #reading"]
    );
    assert_eq!(
        f.ordered("(and (property rating) (sort-by rating desc))"),
        ["skim notes", "<pre>", "read chapter one #reading"]
    );
    // String values sort after numbers are exhausted; NULLs last.
    let o = f.ordered("(and (task TODO DOING) (sort-by status asc))");
    assert_eq!(o[0], "read chapter one #reading");
    assert_eq!(o[1], "skim notes");
    assert_eq!(
        f.ordered("(and (task TODO DONE DOING) (sample 2))").len(),
        2
    );
    assert_eq!(f.ordered("(and (task DONE) (sample 100))").len(), 2);
    assert_eq!(f.ordered("(and (task DONE) (sample 0))").len(), 0);
}

#[test]
fn own_block_is_excluded() {
    let f = fx();
    let res = f.r.query_simple("(task TODO)", &ctx()).expect("run");
    let own = res
        .blocks
        .iter()
        .find(|b| b.title == "standup notes")
        .expect("block");
    let mut c = ctx();
    c.query_block = Some(own.uuid.clone());
    let res = f.r.query_simple("(task TODO)", &c).expect("run");
    assert!(res.blocks.iter().all(|b| b.title != "standup notes"));
    assert_eq!(res.blocks.len(), 4);
    c.limit = Some(2);
    assert_eq!(
        f.r.query_simple("(task TODO)", &c)
            .expect("run")
            .blocks
            .len(),
        2
    );
}

#[test]
fn result_type_rule_and_hydration() {
    let f = fx();
    let r =
        f.r.query_simple("(property status active)", &ctx())
            .expect("run");
    assert_eq!(r.kind, ResultKind::Blocks);
    assert!(
        r.blocks[0]
            .properties
            .iter()
            .any(|(k, v)| k == "status" && v == "active")
    );
    let r =
        f.r.query_simple("(and (page-property type book))", &ctx())
            .expect("run");
    assert_eq!(r.kind, ResultKind::Pages);
    assert_eq!(r.pages[0].original_name, "Alpha");
    // A block-level leaf anywhere makes the whole query a block query.
    assert_eq!(
        f.r.query_simple("(and (page-tags fiction) (task TODO))", &ctx())
            .expect("run")
            .kind,
        ResultKind::Blocks
    );
}

#[test]
fn errors_and_injection_safety() {
    let f = fx();
    let err = f.r.query_simple("(frobnicate x)", &ctx()).unwrap_err();
    assert!(matches!(&err, QueryError::Unsupported(m) if m.contains("frobnicate")));
    assert!(err.to_string().starts_with("unsupported:"));
    assert!(matches!(
        f.r.query_simple("(and (task", &ctx()),
        Err(QueryError::Syntax(_))
    ));
    assert!(matches!(
        f.r.query_simple("(between nowhere today)", &ctx()),
        Err(QueryError::Syntax(_))
    ));
    // Hostile values are bound as parameters: no match, no error, no SQL text.
    let evil = "x' OR 1=1; DROP TABLE blocks; --";
    for q in [
        format!("(page \"{evil}\")"),
        format!("(property \"{evil}\" v)"),
        format!("(property status \"{evil}\")"),
        format!("[[{evil}]]"),
        format!("(page-tags \"{evil}\")"),
        format!("(namespace \"{evil}\")"),
    ] {
        let c = compile_simple(&q, &ctx()).expect("compile");
        assert!(
            !c.sql.contains("DROP") && !c.sql.contains("1=1"),
            "{}",
            c.sql
        );
        let n = c.sql.matches('?').count();
        assert_eq!(n, c.params.len(), "{q}");
        f.r.query_simple(&q, &ctx()).expect("run");
    }
    assert_eq!(
        f.r.query_simple("(task TODO)", &ctx())
            .expect("still there")
            .blocks
            .len(),
        5
    );
}

#[test]
fn parameter_counts_match_for_every_leaf() {
    for q in [
        "[[a]]",
        "\"abc\"",
        "\"ab\"",
        "(task a b)",
        "(priority a)",
        "(property k v)",
        "(property k 3)",
        "(property k true)",
        "(property k)",
        "(between -1d today)",
        "(between created-at -1d now)",
        "(between last-modified-at -1d now)",
        "(page p)",
        "(page-property k v)",
        "(page-property k)",
        "(namespace n)",
        "(page-tags a b)",
        "(all-page-tags)",
        "(and (task a) (sort-by k asc))",
        "(and (task a) (sort-by created-at))",
        "(and (task a) (sample 4))",
    ] {
        let c = compile_simple(q, &ctx()).unwrap_or_else(|e| panic!("{q}: {e}"));
        assert_eq!(c.sql.matches('?').count(), c.params.len(), "{q}: {}", c.sql);
    }
}

#[test]
fn text_filter_without_trigram_index_uses_instr() {
    let env = env();
    let index = env.open();
    let w = writer(&index);
    for (p, t) in FILES {
        w.replace_file(input(p, t)).expect("replace");
    }
    w.set_substring(false).expect("disable");
    let r = index.read_api();
    let res = r.query_simple("\"standup\"", &ctx()).expect("run");
    assert_eq!(res.blocks.len(), 4);
}
