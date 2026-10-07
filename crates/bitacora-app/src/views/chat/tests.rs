//! Chat view tests: the resolver against a real indexed graph (pages loaded on demand, blocks
//! without `id::` addressed through the index uuid, nothing written into the file), the host
//! channel, and the view's behaviour with scripted events.

use std::sync::{Arc, Mutex};

use bitacora_config::EffectiveConfig;
use bitacora_config::pando::GraphConsent;
use bitacora_core::editor::{MemStore, Workspace};
use bitacora_core::graph::PageKey;
use bitacora_core::queue::{CommandQueue, QueueConfig, QueueJoin};
use bitacora_runtime::ai::{
    ApprovalCard, CardKind, CardState, ChatEvent, ContentGuard, DenyReason, EditApplier, Preview,
    PreviewOp, Proposal, QueueEditApplier, RunEnd,
};
use serde_json::json;

use super::*;
use crate::testing::TestGraph;
use crate::ui::testing::{TestAppContext, gpui_test};
use crate::{settings::AppSettings, theme};

fn queue() -> (CommandQueue, QueueJoin) {
    CommandQueue::spawn(
        Workspace::new(),
        Box::new(MemStore::default()),
        QueueConfig {
            debounce: None,
            ..QueueConfig::default()
        },
    )
}

fn guard(exclusions: &[&str]) -> ContentGuard {
    ContentGuard::from_consent(&GraphConsent {
        granted: true,
        exclusions: exclusions.iter().map(|s| (*s).to_owned()).collect(),
        ..GraphConsent::default()
    })
}

/// A graph whose `Notes` page has one block with a persisted `id::` and two without.
fn graph() -> TestGraph {
    TestGraph::new(&[
        (
            "pages/Notes.md",
            "- first\n  id:: 11111111-1111-4111-8111-111111111111\n- plain block\n- another plain\n",
        ),
        ("pages/Other.md", "- other page\n"),
    ])
}

fn index_uuid(g: &TestGraph, text: &str) -> String {
    let row = g
        .handle
        .reader
        .page_by_name("Notes")
        .expect("page")
        .expect("Notes");
    g.handle
        .reader
        .outline(row.id, 0, 100, false)
        .expect("outline")
        .into_iter()
        .find(|b| b.content.trim() == text)
        .map(|b| b.uuid)
        .expect("block")
}

fn proposal(ops: serde_json::Value) -> Proposal {
    Proposal::parse(&json!({"title": "t", "page": "Notes", "ops": ops})).expect("parse")
}

#[test]
fn propose_edit_loads_the_page_and_addresses_blocks_without_id() {
    let g = graph();
    let (q, join) = queue();
    let resolver = Arc::new(AppResolver::new(
        q.clone(),
        g.handle.clone(),
        Arc::new(EffectiveConfig::default()),
    ));
    let applier = QueueEditApplier::new(q.clone()).with_resolver(resolver);
    let plain = index_uuid(&g, "plain block");
    let p = proposal(json!([
        {"op": "update_block", "uuid": plain, "expected_text": "plain block", "text": "edited block"},
        {"op": "insert_block", "text": "added", "after_uuid": plain},
    ]));
    // The page is not loaded in the writer yet; the preview loads it on demand.
    assert!(q.snapshot(&PageKey::from_title("Notes")).is_none());
    let preview = applier.preview(&p).expect("preview");
    assert_eq!(preview.ops.len(), 2);
    assert!(q.snapshot(&PageKey::from_title("Notes")).is_some());
    applier.apply(&p).expect("apply");
    let snap = q.snapshot(&PageKey::from_title("Notes")).expect("snap");
    let texts: Vec<_> = snap.blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "first\nid:: 11111111-1111-4111-8111-111111111111",
            "edited block",
            "added",
            "another plain"
        ]
    );
    // No `id::` was written for the blocks the agent addressed through the index uuid.
    assert!(
        snap.blocks
            .iter()
            .filter(|b| b.text == "edited block" || b.text == "added")
            .all(|b| b.uuid.is_none() && !b.text.contains("id::"))
    );
    // The user's file on disk is untouched until the writer saves.
    let on_disk = std::fs::read_to_string(g.handle.root.join("pages/Notes.md")).expect("read");
    assert!(on_disk.contains("plain block") && !on_disk.contains("edited block"));
    // A page that does not exist is never created for an agent.
    let mut ghost = proposal(json!([{"op": "delete_block", "uuid": plain}]));
    ghost.page = "Nowhere".into();
    assert!(applier.preview(&ghost).is_err());
    assert!(q.snapshot(&PageKey::from_title("Nowhere")).is_none());
    drop(applier);
    drop(q);
    let _ = join.shutdown();
}

#[test]
fn the_host_answers_open_page_and_guards_the_selection() {
    let g = graph();
    let (q, join) = queue();
    let resolver = Arc::new(AppResolver::new(
        q.clone(),
        g.handle.clone(),
        Arc::new(EffectiveConfig::default()),
    ));
    let (tx, rx) = async_channel::unbounded();
    let shared = Arc::new(Mutex::new(Some(guard(&["Secrets"]))));
    let host = Arc::new(AppHost::new(tx, resolver, shared));

    use bitacora_runtime::ai::FrontendHost as _;
    assert!(host.open_page("Nowhere").is_err());
    assert!(host.open_page("Other").is_ok());
    assert!(matches!(
        rx.recv_blocking().expect("request"),
        HostRequest::OpenPage(name) if name == "Other"
    ));

    // `get_selection` waits for the UI thread's answer, then the guard filters it.
    let worker = {
        let host = Arc::clone(&host);
        std::thread::spawn(move || host.get_selection())
    };
    let HostRequest::Selection(reply) = rx.recv_blocking().expect("request") else {
        panic!("expected a selection request");
    };
    reply
        .send(Ok(PageBlocks {
            page: "Secrets".into(),
            file_path: "pages/Secrets.md".into(),
            preamble: None,
            blocks: vec![(None, "hidden".into())],
        }))
        .expect("reply");
    // The host hands the blocks over; the backend runs them through the guard before they reach
    // the agent (`run_get_selection`), which withholds the excluded page.
    let blocks = worker.join().expect("join").expect("selection");
    assert_eq!(blocks.len(), 1);
    let answer = bitacora_runtime::ai::tools::selection_result(&guard(&["Secrets"]), &blocks);
    assert_eq!(answer["withheld"], 1);
    assert!(answer["blocks"].as_array().expect("blocks").is_empty());
    drop(host);
    drop(q);
    let _ = join.shutdown();
}

fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::ui::init(cx);
        theme::install(cx, AppSettings::default(), None);
    });
}

fn edit_card(id: &str) -> ChatEvent {
    ChatEvent::ApprovalRequested(ApprovalCard {
        id: id.into(),
        kind: CardKind::Edit(Preview {
            title: "Tidy the plan".into(),
            page: "Notes".into(),
            ops: vec![PreviewOp {
                kind: "update",
                uuid: None,
                before: Some("old".into()),
                after: Some("new".into()),
            }],
        }),
        timeout_secs: 120,
        remember_tool: Some("propose_edit".into()),
    })
}

#[gpui_test]
fn streamed_events_fold_into_the_transcript_and_render(cx: &mut TestAppContext) {
    setup(cx);
    let (view, cx) = cx.add_window_view(ChatView::new);
    let generation = view.read_with(cx, |v, _| v.generation);
    view.update(cx, |v, cx| {
        v.apply_events(
            generation,
            vec![
                ChatEvent::RunStarted { run_id: "r".into() },
                ChatEvent::State(json!({
                    "model": {"name": "Sonnet"},
                    "tokenUsage": {"promptTokens": 1200, "completionTokens": 300, "contextWindow": 200000}
                })),
                ChatEvent::MessageStart { message_id: "m1".into() },
                ChatEvent::TextDelta { message_id: "m1".into(), delta: "See [[Notes]]".into() },
                ChatEvent::ToolCallStart { call_id: "c1".into(), name: "search".into() },
                ChatEvent::ToolCallArgs { call_id: "c1".into(), delta: "{\"q\":\"x\"}".into() },
            ],
            cx,
        );
    });
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        assert!(v.model().running);
        assert_eq!(v.model().messages[0].text, "See [[Notes]]");
        assert_eq!(
            v.model()
                .state
                .as_ref()
                .and_then(|s| s.model.clone())
                .as_deref(),
            Some("Sonnet")
        );
    });
    // Later deltas touch only their message; the layout of the first is reused.
    view.update(cx, |v, cx| {
        v.apply_events(
            generation,
            vec![
                ChatEvent::ToolCallResult {
                    call_id: "c1".into(),
                    content: "3 pages".into(),
                },
                ChatEvent::MessageStart {
                    message_id: "m2".into(),
                },
                ChatEvent::TextDelta {
                    message_id: "m2".into(),
                    delta: "Done".into(),
                },
                ChatEvent::RunFinished(RunEnd::Finished),
            ],
            cx,
        );
    });
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        assert!(!v.model().running);
        assert_eq!(v.model().messages.len(), 2);
    });
    // Events of an older session are ignored.
    view.update(cx, |v, cx| {
        v.apply_events(
            generation + 7,
            vec![ChatEvent::Error {
                message: "stale".into(),
                fatal: true,
            }],
            cx,
        );
    });
    view.read_with(cx, |v, _| assert!(v.model().error.is_none()));
}

#[gpui_test]
fn closing_denies_pending_approval_cards_with_the_reason(cx: &mut TestAppContext) {
    setup(cx);
    for reason in [
        DenyReason::PanelClosed,
        DenyReason::ThreadSwitch,
        DenyReason::GraphSwitch,
        DenyReason::Quit,
    ] {
        let (view, cx) = cx.add_window_view(ChatView::new);
        let generation = view.read_with(cx, |v, _| v.generation);
        view.update(cx, |v, cx| {
            v.apply_events(generation, vec![edit_card("call-1")], cx)
        });
        cx.run_until_parked();
        view.read_with(cx, |v, _| {
            assert_eq!(v.model().cards[0].state, CardState::Pending);
        });
        view.update(cx, |v, cx| v.close_session(reason, cx));
        cx.run_until_parked();
        view.read_with(cx, |v, _| {
            assert_eq!(v.model().cards[0].state, CardState::Denied(Some(reason)));
            assert!(!v.model().running);
        });
    }
}

#[gpui_test]
fn context_chips_attach_detach_and_report_missing_context(cx: &mut TestAppContext) {
    setup(cx);
    let (view, cx) = cx.add_window_view(ChatView::new);
    let page = PageBlocks {
        page: "Notes".into(),
        file_path: "pages/Notes.md".into(),
        preamble: None,
        blocks: vec![(None, "a".into()), (None, "b".into())],
    };
    // Nothing to attach yet: a hint, no chip.
    view.update(cx, |v, cx| v.attach_selection(cx));
    view.read_with(cx, |v, _| {
        assert!(v.contexts().is_empty());
        assert!(v.notice.is_some());
    });
    let context = ChatContext {
        page: Some((page.clone(), false)),
        selection: Some(PageBlocks {
            blocks: vec![(None, "b".into())],
            ..page.clone()
        }),
    };
    view.update(cx, |v, _| {
        v.set_context_provider(std::rc::Rc::new(move |_| context.clone()));
    });
    view.update(cx, |v, cx| {
        v.attach_page(cx);
        v.attach_selection(cx);
        v.attach_page(cx); // attaching the same page again replaces it
    });
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        let labels: Vec<_> = v.contexts().iter().map(ContextItem::label).collect();
        assert_eq!(labels, ["Notes (1)", "Notes"]);
    });
    view.update(cx, |v, cx| v.detach(0, cx));
    view.read_with(cx, |v, _| assert_eq!(v.contexts().len(), 1));
}

#[gpui_test]
fn sending_without_a_session_reports_it_and_keeps_the_text(cx: &mut TestAppContext) {
    setup(cx);
    let (view, cx) = cx.add_window_view(ChatView::new);
    let text = "what did I plan?";
    view.update_in(cx, |v, window, cx| {
        v.composer.update(cx, |c, cx| c.set_value(text, window, cx));
        v.submit(window, cx);
    });
    cx.run_until_parked();
    view.read_with(cx, |v, cx| {
        assert!(matches!(v.phase(), Phase::Unavailable(_)));
        assert!(v.model().messages.is_empty(), "nothing was sent");
        assert_eq!(v.composer.read(cx).value().to_string(), text);
    });
}

#[test]
fn thread_times_are_short() {
    assert_eq!(short_time("2026-10-07T14:03:59Z"), "2026-10-07 14:03");
    assert_eq!(short_time("now"), "now");
}

#[gpui_test]
fn remember_my_decision_is_a_toggle_on_a_pending_card(cx: &mut TestAppContext) {
    setup(cx);
    let (view, cx) = cx.add_window_view(ChatView::new);
    let generation = view.read_with(cx, |v, _| v.generation);
    view.update(cx, |v, cx| {
        v.apply_events(generation, vec![edit_card("call-1")], cx)
    });
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        assert_eq!(
            v.model().cards[0].card.remember_tool.as_deref(),
            Some("propose_edit")
        );
        assert!(!v.is_remembering("call-1"));
    });
    view.update(cx, |v, cx| v.toggle_remember("call-1", cx));
    view.read_with(cx, |v, _| assert!(v.is_remembering("call-1")));
    view.update(cx, |v, cx| v.toggle_remember("call-1", cx));
    view.read_with(cx, |v, _| assert!(!v.is_remembering("call-1")));
}
