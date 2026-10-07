//! `#[gpui::test]` suite of the query and embed widgets (BIT-US-0102, BIT-US-0104).

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use bitacora_core::graph::PageKey;
use bitacora_core::queue::Source;
use bitacora_runtime::{RuntimeConfig, Session};

use super::{EmbedBlock, Host, QueryBlock};
use crate::data::{GraphHandle, ViewSettings};
use crate::editor::Caret;
use crate::nav::OpenIn;
use crate::render::embed::{Chain, Refusal};
use crate::render::inline::NavTarget;
use crate::render::query::{Body, Failure, Scope};
use crate::render::widget::{EmbedTarget, QueryKind, QueryProps, QuerySpec};
use crate::session::SessionLink;
use crate::settings::AppSettings;
use crate::testing::TestGraph;
use crate::theme;
use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
use crate::ui::{App, Entity};
use crate::views::block_view::Nav;

const UUID: &str = "6500c1a4-0000-4000-8000-000000000001";

fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::ui::init(cx);
        theme::install(cx, AppSettings::default(), None);
        crate::keymap::load_with_user(cx, None).expect("keymap");
    });
}

fn nav() -> Nav {
    Rc::new(|_: NavTarget, _: OpenIn, _: &mut App| {})
}

fn simple(src: &str) -> QuerySpec {
    QuerySpec {
        kind: QueryKind::Simple,
        source: src.to_owned(),
        props: QueryProps::default(),
    }
}

fn advanced(src: &str) -> QuerySpec {
    QuerySpec {
        kind: QueryKind::Advanced,
        source: src.to_owned(),
        props: QueryProps::default(),
    }
}

fn settle<V: 'static>(cx: &mut VisualTestContext, view: &Entity<V>, done: impl Fn(&V) -> bool) {
    cx.executor().allow_parking();
    for _ in 0..400 {
        cx.run_until_parked();
        if view.read_with(cx, |v, _| done(v)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the view did not settle");
}

fn query_view<'a>(
    cx: &'a mut TestAppContext,
    g: &TestGraph,
    spec: QuerySpec,
) -> (Entity<QueryBlock>, &'a mut VisualTestContext) {
    setup(cx);
    let handle = g.handle.clone();
    let (view, cx) = cx.add_window_view(move |_, cx| {
        QueryBlock::new(handle, spec, Scope::default(), nav(), None, cx)
    });
    settle(cx, &view, QueryBlock::is_settled);
    (view, cx)
}

const PAGES: &[(&str, &str)] = &[
    (
        "pages/Alpha.md",
        "- TODO write docs\n  status:: open\n  weight:: 10\n- DONE shipped\n- TODO fix bug\n  status:: blocked\n  weight:: 9\n",
    ),
    (
        "pages/Beta.md",
        "- TODO review\n  status:: open\n  weight:: 100\n\t- nested note\n",
    ),
];

#[gpui_test]
fn simple_queries_list_blocks_grouped_by_page(cx: &mut TestAppContext) {
    let g = TestGraph::new(PAGES);
    let (view, cx) = query_view(cx, &g, simple("(task TODO)"));
    let n = view.read_with(cx, |v, _| match v.body() {
        Some(Body::Blocks(b)) => b.len(),
        _ => 0,
    });
    assert_eq!(n, 3);
    assert!(!view.read_with(cx, |v, _| v.is_table()));
    assert!(!view.read_with(cx, |v, _| v.is_collapsed()));
    view.update(cx, |v, cx| v.toggle_collapsed(cx));
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.is_collapsed()));
    view.update(cx, |v, cx| v.toggle_collapsed(cx));
    assert!(!view.read_with(cx, |v, _| v.is_collapsed()));
}

#[gpui_test]
fn table_view_honours_properties_sort_and_the_column_picker(cx: &mut TestAppContext) {
    let g = TestGraph::new(PAGES);
    let spec = QuerySpec {
        props: QueryProps {
            table: Some(true),
            properties: Some(vec!["block".into(), "status".into(), "weight".into()]),
            sort_by: Some("weight".into()),
            sort_desc: Some(true),
        },
        ..simple("(task TODO)")
    };
    let (view, cx) = query_view(cx, &g, spec);
    assert!(view.read_with(cx, |v, _| v.is_table()));
    let t = view.read_with(cx, |v, _| v.table_model()).expect("table");
    assert_eq!(t.columns, ["block", "status", "weight"]);
    let weights: Vec<&str> = t.rows.iter().map(|r| r.cells[2].as_str()).collect();
    assert_eq!(weights, ["100", "10", "9"]);
    // A click on a header sorts by it: ascending first, then descending.
    view.update(cx, |v, cx| v.sort_by("status", cx));
    let t = view.read_with(cx, |v, _| v.table_model()).expect("table");
    let status: Vec<&str> = t.rows.iter().map(|r| r.cells[1].as_str()).collect();
    assert_eq!(status, ["blocked", "open", "open"]);
    view.update(cx, |v, cx| v.sort_by("status", cx));
    let t = view.read_with(cx, |v, _| v.table_model()).expect("table");
    assert_eq!(t.rows[0].cells[1], "open");
    // The picker adds and removes columns, keeping the available order.
    view.update(cx, |v, cx| v.toggle_column("page", cx));
    let t = view.read_with(cx, |v, _| v.table_model()).expect("table");
    assert_eq!(t.columns, ["block", "page", "status", "weight"]);
    view.update(cx, |v, cx| v.toggle_column("weight", cx));
    let t = view.read_with(cx, |v, _| v.table_model()).expect("table");
    assert_eq!(t.columns, ["block", "page", "status"]);
    // Back to the list and again.
    view.update(cx, |v, cx| v.toggle_table(cx));
    assert!(!view.read_with(cx, |v, _| v.is_table()));
    view.update(cx, |v, cx| v.toggle_picker(cx));
    cx.run_until_parked();
}

#[gpui_test]
fn errors_unsupported_constructs_and_warnings_show_without_breaking(cx: &mut TestAppContext) {
    let g = TestGraph::new(PAGES);
    let (bad, cx) = query_view(cx, &g, simple("(and [[a]]"));
    assert!(matches!(
        bad.read_with(cx, |v, _| v.failure().cloned()),
        Some(Failure::Syntax(_))
    ));
    let unsupported = advanced("{:query [:find ?b :where [?b :block/nonexistent-attr ?x]]}");
    let (view, cx) = query_view(cx, &g, unsupported);
    assert!(matches!(
        view.read_with(cx, |v, _| v.failure().cloned()),
        Some(Failure::Unsupported(_))
    ));
    let warned = advanced(
        "{:title \"Open\" :query [:find (pull ?b [*]) :where [?b :block/marker \"TODO\"]] :result-transform (fn [r] r)}",
    );
    let (view, cx) = query_view(cx, &g, warned);
    assert!(view.read_with(cx, |v, _| v.failure().is_none()));
    let n = view.read_with(cx, |v, _| v.body().map_or(0, Body::len));
    assert_eq!(n, 3);
}

#[gpui_test]
fn advanced_aggregates_become_a_table(cx: &mut TestAppContext) {
    let g = TestGraph::new(PAGES);
    let spec =
        advanced("{:title \"Count\" :query [:find (count ?b) :where [?b :block/marker \"TODO\"]]}");
    let (view, cx) = query_view(cx, &g, spec);
    assert!(view.read_with(cx, |v, _| v.is_table()));
    let t = view.read_with(cx, |v, _| v.table_model()).expect("table");
    assert_eq!(t.rows.len(), 1);
    assert_eq!(t.rows[0].cells[0], "3");
}

#[gpui_test]
fn results_refresh_when_the_index_changes(cx: &mut TestAppContext) {
    let g = TestGraph::new(PAGES);
    let (view, cx) = query_view(cx, &g, simple("(task TODO)"));
    assert_eq!(view.read_with(cx, |v, _| v.body().map_or(0, Body::len)), 3);
    let event = g.rewrite(
        "pages/Beta.md",
        "- TODO review\n- TODO more\n- TODO again\n",
    );
    assert!(super::query_block::is_relevant(&event));
    // The widget was drawn, so it re-runs after the debounce.
    view.update(cx, |v, cx| v.on_index_event(&event, cx));
    cx.executor()
        .advance_clock(super::query_block::REFRESH_DEBOUNCE + Duration::from_millis(10));
    settle(cx, &view, |v| v.body().map_or(0, Body::len) == 5);
}

#[test]
fn a_file_replaced_without_changes_is_not_relevant() {
    let quiet = bitacora_index::IndexEvent::FileReplaced {
        file_id: 1,
        path: "pages/A.md".into(),
        page_ids_touched: Vec::new(),
        block_uuids_added: Vec::new(),
        block_uuids_removed: Vec::new(),
    };
    assert!(!super::query_block::is_relevant(&quiet));
    assert!(super::query_block::is_relevant(
        &bitacora_index::IndexEvent::BulkFinished
    ));
}

// ---- embeds ---------------------------------------------------------------------------

struct Env {
    graph: tempfile::TempDir,
    _data: tempfile::TempDir,
    session: Option<Session>,
    handle: GraphHandle,
    link: SessionLink,
}

impl Drop for Env {
    fn drop(&mut self) {
        if let Some(s) = self.session.take() {
            let _ = s.shutdown(Duration::from_secs(10));
        }
    }
}

impl Env {
    fn new(files: &[(&str, &str)]) -> Self {
        let graph = tempfile::tempdir().expect("graph");
        let data = tempfile::tempdir().expect("data");
        let root = graph.path();
        std::fs::create_dir_all(root.join("logseq")).expect("logseq");
        std::fs::write(root.join("logseq/config.edn"), "{}").expect("config");
        for (path, content) in files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().expect("parent")).expect("dirs");
            std::fs::write(file, content).expect("write");
        }
        let mut cfg = RuntimeConfig::new(root);
        cfg.data_dir = Some(data.path().to_path_buf());
        cfg.global_config = Some(data.path().join("no-global.edn"));
        cfg.watch = None;
        cfg.debounce = None;
        let session = Session::open(cfg).expect("session");
        let handle = GraphHandle {
            reader: session.read_api(),
            root: session.root().to_path_buf(),
            settings: Arc::new(ViewSettings::from_config(session.config())),
        };
        let link = SessionLink {
            queue: session.queue().clone(),
            config: Arc::new(session.config().clone()),
            mcp_endpoint: None,
            gate: Arc::default(),
            lookup: session.ref_lookup(),
            hybrid: None,
        };
        Self {
            graph,
            _data: data,
            session: Some(session),
            handle,
            link,
        }
    }

    fn host(&self, page: &str, live: bool) -> Host {
        Host {
            handle: self.handle.clone(),
            link: live.then(|| self.link.clone()),
            scope: format!("page:{page}"),
            page: page.to_owned(),
            chain: Chain::page(page),
            ancestors: Vec::new(),
        }
    }

    fn disk(&self, rel: &str) -> String {
        let _ = self.link.queue.flush(Source::Ui).expect("flush");
        std::fs::read_to_string(self.graph.path().join(rel)).expect("read page")
    }

    fn texts(&self, title: &str) -> Vec<String> {
        self.link
            .queue
            .snapshot(&PageKey::from_title(title))
            .map(|s| s.blocks.iter().map(|b| b.text.clone()).collect())
            .unwrap_or_default()
    }
}

fn embed_view(
    cx: &mut TestAppContext,
    host: Host,
    target: EmbedTarget,
) -> (Entity<EmbedBlock>, &mut VisualTestContext) {
    setup(cx);
    let (view, cx) =
        cx.add_window_view(move |_, cx| EmbedBlock::new(host, target, nav(), None, "k", cx));
    settle(cx, &view, |v| {
        v.refusal().is_some() || !v.crumb_labels().is_empty()
    });
    (view, cx)
}

const SOURCE: &str = "pages/Source.md";
const SOURCE_TEXT: &str =
    "- parent\n  id:: 6500c1a4-0000-4000-8000-000000000001\n\t- child\n- other\n";

fn row_texts(view: &Entity<EmbedBlock>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |v, cx| {
        v.rows(cx)
            .iter()
            .map(|r| r.block.title.text.clone())
            .collect()
    })
}

#[gpui_test]
fn block_embeds_show_the_subtree_with_a_breadcrumb_and_edit_the_source(cx: &mut TestAppContext) {
    let host_text = format!("- {{{{embed (({UUID}))}}}}\n");
    let env = Env::new(&[(SOURCE, SOURCE_TEXT), ("pages/Host.md", &host_text)]);
    let (view, cx) = embed_view(
        cx,
        env.host("Host", true),
        EmbedTarget::Block(UUID.to_owned()),
    );
    settle(cx, &view, EmbedBlock::is_live);
    assert_eq!(view.read_with(cx, |v, _| v.crumb_labels()), ["Source"]);
    assert_eq!(row_texts(&view, cx), ["parent", "child"]);
    // Editing inside the embed goes through core and changes the source page only.
    let ed = view.read_with(cx, |v, _| v.editor().cloned().expect("nested editor"));
    let id = ed.read_with(cx, |e, _| e.block_ids()[1]);
    ed.update_in(cx, |e, window, cx| e.enter(id, Caret::End, window, cx));
    cx.simulate_input("!");
    ed.update(cx, |e, cx| {
        e.flush(cx);
    });
    assert_eq!(env.texts("Source")[1], "child!");
    assert_eq!(
        env.disk(SOURCE),
        "- parent\n  id:: 6500c1a4-0000-4000-8000-000000000001\n\t- child!\n- other\n"
    );
    assert_eq!(env.disk("pages/Host.md"), host_text);
    // The embed shows the edit once the block leaves edit mode.
    ed.update(cx, |e, cx| e.exit_edit(cx));
    assert_eq!(row_texts(&view, cx), ["parent", "child!"]);
}

#[gpui_test]
fn page_embeds_show_the_whole_page_and_fold(cx: &mut TestAppContext) {
    let env = Env::new(&[
        (SOURCE, SOURCE_TEXT),
        ("pages/Host.md", "- {{embed [[Source]]}}\n"),
    ]);
    let (view, cx) = embed_view(
        cx,
        env.host("Host", true),
        EmbedTarget::Page("source".into()),
    );
    settle(cx, &view, EmbedBlock::is_live);
    assert_eq!(row_texts(&view, cx), ["parent", "child", "other"]);
    assert!(!view.read_with(cx, |v, _| v.is_collapsed()));
    view.update(cx, |v, cx| v.toggle_collapsed(cx));
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.is_collapsed()));
}

#[gpui_test]
fn embeds_of_the_host_page_or_an_ancestor_block_are_circular(cx: &mut TestAppContext) {
    let env = Env::new(&[(SOURCE, SOURCE_TEXT)]);
    let (view, cx) = embed_view(
        cx,
        env.host("Source", true),
        EmbedTarget::Page("Source".into()),
    );
    assert_eq!(
        view.read_with(cx, |v, _| v.refusal()),
        Some(Refusal::Circular)
    );
    let mut host = env.host("Source", true);
    host.ancestors = vec![format!("block:{UUID}")];
    let (view, cx) = embed_view(cx, host, EmbedTarget::Block(UUID.to_owned()));
    assert_eq!(
        view.read_with(cx, |v, _| v.refusal()),
        Some(Refusal::Circular)
    );
}

#[gpui_test]
fn nested_embeds_stop_at_the_depth_limit(cx: &mut TestAppContext) {
    // P0 embeds P1 embeds ... P7: the chain is cut at depth 5.
    let mut files: Vec<(String, String)> = (0..8)
        .map(|n| {
            let body = if n < 7 {
                format!("- level {n}\n- {{{{embed [[P{}]]}}}}\n", n + 1)
            } else {
                "- last\n".to_owned()
            };
            (format!("pages/P{n}.md"), body)
        })
        .collect();
    files.push(("pages/Top.md".into(), "- top\n".into()));
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let env = Env::new(&refs);
    let (view, cx) = embed_view(cx, env.host("Top", false), EmbedTarget::Page("P0".into()));
    // Every level creates the next one while it is drawn.
    for _ in 0..60 {
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(5));
    }
    let all = cx.update(|_, app| super::embeds(app));
    let deep = all
        .into_iter()
        .filter(|e| e.read_with(cx, |v, _| v.refusal() == Some(Refusal::TooDeep)))
        .count();
    assert_eq!(deep, 1, "exactly one embed hits the limit");
    assert_eq!(
        view.read_with(cx, |v, _| v.refusal()),
        None,
        "the outermost embed is drawn"
    );
}

#[gpui_test]
fn read_only_embeds_reload_on_index_events_of_the_source_file(cx: &mut TestAppContext) {
    let g = TestGraph::new(&[(SOURCE, SOURCE_TEXT)]);
    setup(cx);
    let host = Host {
        handle: g.handle.clone(),
        link: None,
        scope: "page:Host".into(),
        page: "Host".into(),
        chain: Chain::page("Host"),
        ancestors: Vec::new(),
    };
    let (view, cx) = cx.add_window_view(move |_, cx| {
        EmbedBlock::new(
            host,
            EmbedTarget::Page("Source".into()),
            nav(),
            None,
            "k",
            cx,
        )
    });
    settle(cx, &view, |v| !v.crumb_labels().is_empty());
    assert!(!view.read_with(cx, |v, _| v.is_live()));
    assert_eq!(row_texts(&view, cx), ["parent", "child", "other"]);
    let event = g.rewrite(SOURCE, "- parent\n- brand new\n");
    view.update(cx, |v, cx| v.on_index_event(&event, cx));
    cx.executor()
        .advance_clock(super::embed_block::REFRESH_DEBOUNCE + Duration::from_millis(10));
    settle(cx, &view, |_| true);
    for _ in 0..200 {
        cx.run_until_parked();
        if row_texts(&view, cx) == ["parent", "brand new"] {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(row_texts(&view, cx), ["parent", "brand new"]);
}

#[gpui_test]
fn missing_targets_say_so(cx: &mut TestAppContext) {
    let env = Env::new(&[(SOURCE, SOURCE_TEXT)]);
    setup(cx);
    let (view, cx) = cx.add_window_view(|_, cx| {
        EmbedBlock::new(
            env.host("Host", false),
            EmbedTarget::Page("No such page".into()),
            nav(),
            None,
            "k",
            cx,
        )
    });
    cx.executor().allow_parking();
    for _ in 0..100 {
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(view.read_with(cx, |v, _| v.crumb_labels().is_empty()
        && v.refusal().is_none()));
}
