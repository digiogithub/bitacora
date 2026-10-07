//! Suggestion chips of the right panel's Context tab (BIT-US-0152): related pages, missing
//! links, tags and next actions an agent recommends for the page on screen.
//!
//! Suggestions are shown in the amber AI treatment until the user decides. Accepting a link or a
//! tag is an ordinary edit through the core command queue (single writer): a link rewrites the
//! quoted span of one block and goes through the same validated, audited, undoable path as the
//! chat's approved edits; a tag merges into the page's `tags::` property. Both refuse to touch a
//! block that changed since the suggestion was made. Dismissing hides the suggestion for good on
//! this machine ([`DismissStore`]), for the same content only. Nothing runs without the user (or
//! the opt-in auto mode, driven by the workspace) and nothing is shown for pages the consent
//! excludes: the backend drops those before they get here.

use std::collections::HashSet;
use std::sync::Arc;

use bitacora_core::editor::Cmd;
use bitacora_core::graph::PageKey;
use bitacora_core::queue::Source;
use bitacora_runtime::ai::recommend::{LinkSuggestion, RelatedPage, TagSuggestion};
use bitacora_runtime::ai::{
    EditApplier as _, EditOp, PageResolver as _, Proposal, RecommendOutcome, RecommendRequest,
    run_recommend,
};
use rust_i18n::t;

use super::dismiss::{self, DismissStore, Kind};
use super::{AiContext, AiGate, TagEdit, apply_link, merge_tags};
use crate::render::inline::NavTarget;
use crate::ui::theme::{ActiveBitacoraTheme as _, TypeStyleExt as _};
use crate::ui::{
    AnyElement, Context, EventEmitter, FluentBuilder as _, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _, Task,
    Window, div, h_flex, v_flex,
};
use crate::views::chat::AppResolver;
use crate::views::kit::{Button, Card, Chip, ChipTone, Glyph, IconButton, Overline, glyph};

/// What the chips ask of their host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestEvent {
    /// Open a page.
    Navigate(NavTarget),
    /// A run ended; `ok` is false for a failure.
    Finished {
        /// The run produced suggestions.
        ok: bool,
    },
    /// An accepted suggestion edited this page.
    Edited(String),
}

/// One suggestion on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A missing `[[link]]`.
    Link(LinkSuggestion),
    /// A tag to add.
    Tag(TagSuggestion),
    /// A page worth reading.
    Related(RelatedPage),
    /// A next action.
    Action(String),
}

impl Item {
    /// The dismissal key of this suggestion for `page`.
    #[must_use]
    pub fn key(&self, page: &str) -> String {
        match self {
            Item::Link(l) => dismiss::key(Kind::Link, page, &[&l.block_uuid, &l.target, &l.text]),
            Item::Tag(t) => dismiss::key(Kind::Tag, page, &[&t.tag]),
            Item::Related(r) => dismiss::key(Kind::Related, page, &[&r.page]),
            Item::Action(a) => dismiss::key(Kind::Action, page, &[a]),
        }
    }
}

/// Where the run stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunState {
    /// Nothing asked for the page on screen.
    Idle,
    /// A run is in flight.
    Running,
    /// The last run failed.
    Failed(String),
}

/// The chips view.
pub struct SuggestionsView {
    gate: AiGate,
    ctx: Option<AiContext>,
    page: Option<String>,
    run: RunState,
    /// Suggestions and the page they are for.
    result: Option<(String, RecommendOutcome)>,
    /// Keys accepted in this session (their suggestion is gone).
    applied: HashSet<String>,
    /// Keys being applied right now.
    busy: HashSet<String>,
    /// Why the last accept failed.
    notice: Option<String>,
    dismissed: DismissStore,
    generation: u64,
    task: Option<Task<()>>,
    apply_tasks: Vec<Task<()>>,
}

impl std::fmt::Debug for SuggestionsView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SuggestionsView")
            .field("page", &self.page)
            .field("run", &self.run)
            .finish_non_exhaustive()
    }
}

impl EventEmitter<SuggestEvent> for SuggestionsView {}

impl Default for SuggestionsView {
    fn default() -> Self {
        Self::new()
    }
}

impl SuggestionsView {
    /// A hidden view (no graph, feature off).
    #[must_use]
    pub fn new() -> Self {
        Self {
            gate: AiGate::default(),
            ctx: None,
            page: None,
            run: RunState::Idle,
            result: None,
            applied: HashSet::new(),
            busy: HashSet::new(),
            notice: None,
            dismissed: DismissStore::memory(),
            generation: 0,
            task: None,
            apply_tasks: Vec::new(),
        }
    }

    /// The graph the chips work on (`None` when it is closed: everything resets).
    pub fn set_context(&mut self, ctx: Option<AiContext>, cx: &mut Context<Self>) {
        if ctx.is_none() {
            self.reset(cx);
        }
        self.ctx = ctx;
        cx.notify();
    }

    /// Feature switch, consent and connection. Turning the feature off removes the chips.
    pub fn set_gate(&mut self, gate: AiGate, cx: &mut Context<Self>) {
        if self.gate == gate {
            return;
        }
        if !gate.enabled {
            self.reset(cx);
        }
        self.gate = gate;
        cx.notify();
    }

    /// Where dismissals are remembered for the open graph.
    pub fn set_dismiss_store(&mut self, store: DismissStore, cx: &mut Context<Self>) {
        self.dismissed = store;
        cx.notify();
    }

    /// The page on screen. Suggestions belong to one page: a different page drops them.
    pub fn set_page(&mut self, page: Option<String>, cx: &mut Context<Self>) {
        if self.page == page {
            return;
        }
        self.page = page;
        self.reset(cx);
        cx.notify();
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        let was_running = self.run == RunState::Running;
        self.generation += 1;
        self.task = None;
        self.run = RunState::Idle;
        self.result = None;
        self.notice = None;
        if was_running {
            cx.emit(SuggestEvent::Finished { ok: false });
        }
    }

    /// The chips are offered (the feature is on for this graph and a page is open).
    #[must_use]
    pub fn visible(&self) -> bool {
        self.gate.enabled && self.page.is_some()
    }

    /// The run state.
    #[must_use]
    pub fn run_state(&self) -> &RunState {
        &self.run
    }

    /// The suggestions to show: those for the page on screen that were neither dismissed nor
    /// accepted.
    #[must_use]
    pub fn items(&self) -> Vec<Item> {
        let Some((page, outcome)) = &self.result else {
            return Vec::new();
        };
        if self.page.as_deref() != Some(page.as_str()) {
            return Vec::new();
        }
        let s = &outcome.suggestions;
        let all = s
            .related_pages
            .iter()
            .cloned()
            .map(Item::Related)
            .chain(s.link_suggestions.iter().cloned().map(Item::Link))
            .chain(s.tag_suggestions.iter().cloned().map(Item::Tag))
            .chain(s.next_actions.iter().cloned().map(Item::Action));
        all.filter(|i| {
            let k = i.key(page);
            !self.dismissed.is_dismissed(&k) && !self.applied.contains(&k)
        })
        .collect()
    }

    /// Why a run cannot start now, `None` when it can.
    #[must_use]
    pub fn unavailable_reason(&self) -> Option<String> {
        if !self.gate.enabled || self.page.is_none() {
            Some(t!("ai.suggest.off").to_string())
        } else if !self.gate.connected {
            Some(t!("ai.not_connected").to_string())
        } else if self.ctx.is_none() {
            Some(t!("ai.no_graph").to_string())
        } else {
            None
        }
    }

    /// Asks for suggestions for the page on screen.
    pub fn request(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.page.clone() else {
            return;
        };
        self.request_for(&page, cx);
    }

    /// Asks for suggestions for `page` (the auto mode names the page it waited on). Ignored
    /// when `page` is not the one on screen or a run is in flight.
    pub fn request_for(&mut self, page: &str, cx: &mut Context<Self>) {
        if self.run == RunState::Running || self.page.as_deref() != Some(page) {
            // The caller (the auto mode) waits for an end to this request.
            cx.emit(SuggestEvent::Finished { ok: false });
            return;
        }
        if let Some(message) = self.unavailable_reason() {
            self.run = RunState::Failed(message);
            cx.emit(SuggestEvent::Finished { ok: false });
            cx.notify();
            return;
        }
        let Some(ctx) = self.ctx.clone() else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        self.run = RunState::Running;
        self.notice = None;
        let page = page.to_owned();
        let deps = ctx.session.run(|s| s.recommend_deps());
        self.task = Some(cx.spawn(async move |this, cx| {
            let request = RecommendRequest { page: page.clone() };
            let outcome: Result<RecommendOutcome, String> = match deps.recv().await {
                Ok(Ok(deps)) => {
                    let task = this.update(cx, |_, cx| {
                        crate::tokio_bridge::spawn(cx, async move {
                            run_recommend(&deps, &request).await
                        })
                    });
                    match task {
                        Ok(task) => match task.await {
                            Ok(result) => result.map_err(|e| e.to_string()),
                            Err(e) => Err(e.to_string()),
                        },
                        Err(_) => return,
                    }
                }
                Ok(Err(e)) => Err(e.to_string()),
                Err(_) => Err(t!("ai.no_graph").to_string()),
            };
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.finish(&page, outcome, cx);
                }
            });
        }));
        cx.notify();
    }

    /// Shows the end of a run. Public so the result path can be exercised without Pando.
    pub fn finish(
        &mut self,
        page: &str,
        outcome: Result<RecommendOutcome, String>,
        cx: &mut Context<Self>,
    ) {
        self.task = None;
        let ok = outcome.is_ok();
        match outcome {
            Ok(outcome) => {
                self.run = RunState::Idle;
                self.result = Some((page.to_owned(), outcome));
            }
            Err(message) => self.run = RunState::Failed(message),
        }
        cx.emit(SuggestEvent::Finished { ok });
        cx.notify();
    }

    /// Hides a suggestion for good (machine-local, same content only).
    pub fn dismiss(&mut self, item: &Item, cx: &mut Context<Self>) {
        if let Some(page) = self.page.clone() {
            self.dismissed.dismiss(item.key(&page));
            cx.notify();
        }
    }

    /// Accepts a link or tag suggestion: one undoable edit through the command queue. Pages
    /// and next actions have nothing to accept.
    pub fn accept(&mut self, item: &Item, cx: &mut Context<Self>) {
        let (Some(ctx), Some(page)) = (self.ctx.clone(), self.page.clone()) else {
            return;
        };
        if !matches!(item, Item::Link(_) | Item::Tag(_)) {
            return;
        }
        let key = item.key(&page);
        if !self.busy.insert(key.clone()) {
            return;
        }
        self.notice = None;
        let item = item.clone();
        let applier = ctx.session.run(|s| s.agent_edit_applier());
        let resolver = AppResolver::new(
            ctx.link.queue.clone(),
            ctx.graph.clone(),
            Arc::clone(&ctx.link.config),
        );
        let queue = ctx.link.queue.clone();
        let edited = page.clone();
        let task = cx.spawn(async move |this, cx| {
            let applier = applier.recv().await.ok();
            let result = cx
                .background_executor()
                .spawn(async move { perform(&item, &page, applier, &resolver, &queue) })
                .await;
            let _ = this.update(cx, |this, cx| this.accepted(key, edited, result, cx));
        });
        self.apply_tasks.retain(|t| !t.is_ready());
        self.apply_tasks.push(task);
        cx.notify();
    }

    /// Shows the end of an accept. Public so the result path can be exercised without a graph.
    pub fn accepted(
        &mut self,
        key: String,
        page: String,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        self.busy.remove(&key);
        match result {
            Ok(()) => {
                self.applied.insert(key);
                cx.emit(SuggestEvent::Edited(page));
            }
            Err(message) => self.notice = Some(message),
        }
        cx.notify();
    }

    /// The text of the last failed accept.
    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }
}

/// The edit of an accepted suggestion, run on a background thread.
fn perform(
    item: &Item,
    page: &str,
    applier: Option<bitacora_runtime::ai::QueueEditApplier>,
    resolver: &AppResolver,
    queue: &bitacora_core::queue::CommandQueue,
) -> Result<(), String> {
    match item {
        Item::Link(s) => {
            let applier = applier
                .ok_or_else(|| t!("ai.no_graph").to_string())?
                .with_resolver(Arc::new(resolver.clone()));
            if !resolver.ensure_loaded(page) {
                return Err(t!("ai.suggest.page_unavailable").to_string());
            }
            let current = resolver
                .indexed_blocks(page)
                .into_iter()
                .find(|b| b.uuid.eq_ignore_ascii_case(&s.block_uuid))
                .ok_or_else(|| t!("ai.suggest.stale").to_string())?;
            let linked =
                apply_link(&current.text, s).ok_or_else(|| t!("ai.suggest.stale").to_string())?;
            // `id::` and `collapsed::` are managed by Bitacora: the proposal carries the text
            // without them and the applier puts the block's own values back.
            let text = ["id", "collapsed"].iter().fold(linked, |t, key| {
                bitacora_markdown::edit::properties::remove_property(&t, key)
            });
            let proposal = Proposal {
                title: t!("ai.suggest.link_title", target = s.target.as_str()).to_string(),
                page: page.to_owned(),
                ops: vec![EditOp::UpdateBlock {
                    uuid: s.block_uuid.clone(),
                    expected_text: current.text,
                    text,
                }],
            };
            applier
                .apply(&proposal)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        Item::Tag(tag) => {
            if !resolver.ensure_loaded(page) {
                return Err(t!("ai.suggest.page_unavailable").to_string());
            }
            let key = PageKey::from_title(page);
            let snap = queue
                .snapshot(&key)
                .ok_or_else(|| t!("ai.suggest.page_unavailable").to_string())?;
            match merge_tags(snap.preamble.as_deref(), &tag.tag) {
                TagEdit::Already => Ok(()),
                TagEdit::Unsupported => Err(t!("ai.suggest.tags_front_matter").to_string()),
                TagEdit::Set(value) => queue
                    .run(
                        Source::Ui,
                        "Add tag",
                        Cmd::SetPageProperty {
                            page: key,
                            key: "tags".to_owned(),
                            value,
                        },
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
            }
        }
        Item::Related(_) | Item::Action(_) => Ok(()),
    }
}

impl SuggestionsView {
    fn row_actions(&self, ix: usize, item: &Item, cx: &mut Context<Self>) -> AnyElement {
        let bt = cx.bitacora().clone();
        let acceptable = matches!(item, Item::Link(_) | Item::Tag(_));
        let busy = self
            .page
            .as_deref()
            .is_some_and(|p| self.busy.contains(&item.key(p)));
        let (accept_item, dismiss_item) = (item.clone(), item.clone());
        h_flex()
            .flex_shrink_0()
            .gap(bt.metrics.space[2])
            .when(acceptable, |row| {
                row.child(
                    IconButton::new(("suggest-accept", ix), Glyph::Check)
                        .small()
                        .disabled(busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.accept(&accept_item, cx);
                        })),
                )
            })
            .child(
                IconButton::new(("suggest-dismiss", ix), Glyph::Close)
                    .small()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dismiss(&dismiss_item, cx);
                    })),
            )
            .into_any_element()
    }

    fn row(&self, ix: usize, item: &Item, cx: &mut Context<Self>) -> AnyElement {
        let bt = cx.bitacora().clone();
        let m = &bt.metrics;
        let reason = |text: &str| {
            div()
                .text_color(bt.colors.muted)
                .type_style(&bt.type_scale.caption)
                .child(SharedString::from(text.to_owned()))
        };
        let body: AnyElement = match item {
            Item::Link(l) => v_flex()
                .flex_1()
                .min_w_0()
                .gap(m.space[1])
                .child(
                    h_flex()
                        .flex_wrap()
                        .items_center()
                        .gap(m.space[2])
                        .child(Chip::new(l.text.clone()).tone(ChipTone::Outline))
                        .child(glyph(Glyph::ChevronRight, m.icon_sm, bt.colors.ai, cx))
                        .child(Chip::new(format!("[[{}]]", l.target)).tone(ChipTone::Ai)),
                )
                .into_any_element(),
            Item::Tag(tag) => v_flex()
                .flex_1()
                .min_w_0()
                .gap(m.space[1])
                .child(h_flex().child(Chip::new(format!("#{}", tag.tag)).tone(ChipTone::Ai)))
                .when(!tag.reason.is_empty(), |c| c.child(reason(&tag.reason)))
                .into_any_element(),
            Item::Related(r) => {
                let target = NavTarget::Page(r.page.clone());
                v_flex()
                    .id(("suggest-related", ix))
                    .flex_1()
                    .min_w_0()
                    .gap(m.space[1])
                    .cursor_pointer()
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(SuggestEvent::Navigate(target.clone()));
                    }))
                    .child(
                        h_flex()
                            .gap(m.space[2])
                            .child(glyph(Glyph::File, m.icon_sm, bt.colors.ai, cx))
                            .child(
                                div()
                                    .truncate()
                                    .text_color(bt.colors.text)
                                    .type_style(&bt.type_scale.ui)
                                    .child(SharedString::from(r.page.clone())),
                            ),
                    )
                    .when(!r.reason.is_empty(), |c| c.child(reason(&r.reason)))
                    .into_any_element()
            }
            Item::Action(a) => div()
                .flex_1()
                .min_w_0()
                .text_color(bt.colors.text)
                .type_style(&bt.type_scale.ui_small)
                .child(SharedString::from(a.clone()))
                .into_any_element(),
        };
        h_flex()
            .id(("suggestion", ix))
            .gap(m.space[3])
            .items_start()
            .child(body)
            .child(self.row_actions(ix, item, cx))
            .into_any_element()
    }
}

impl Render for SuggestionsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible() {
            return div().into_any_element();
        }
        let bt = cx.bitacora().clone();
        let m = &bt.metrics;
        let items = self.items();
        let has_result = self.result.is_some() && self.run != RunState::Running;
        let usable = self.unavailable_reason().is_none();
        let mut header = h_flex()
            .gap(m.space[3])
            .items_center()
            .child(glyph(Glyph::Sparkle, m.icon_sm, bt.colors.ai, cx))
            .child(
                div()
                    .flex_1()
                    .child(Overline::new(t!("ai.suggest.title").to_string()).count(items.len())),
            );
        header = header.child(
            Button::new("suggest-run")
                .label(if has_result {
                    t!("ai.suggest.refresh").to_string()
                } else {
                    t!("ai.suggest.run").to_string()
                })
                .ai()
                .compact()
                .disabled(!usable || self.run == RunState::Running)
                .on_click(cx.listener(|this, _, _, cx| this.request(cx))),
        );
        let note = |text: String, color| {
            div()
                .text_color(color)
                .type_style(&bt.type_scale.ui_small)
                .child(text)
        };
        let mut body = v_flex().gap(m.space[4]);
        match &self.run {
            RunState::Running => {
                body = body.child(note(t!("ai.suggest.running").to_string(), bt.colors.text_2));
            }
            RunState::Failed(message) => {
                body = body.child(
                    note(message.clone(), bt.colors.warn)
                        .id("suggest-error")
                        .debug_selector(|| "suggest-error".to_string()),
                );
            }
            RunState::Idle if !has_result => {
                let hint = self
                    .unavailable_reason()
                    .unwrap_or_else(|| t!("ai.suggest.hint").to_string());
                body = body.child(note(hint, bt.colors.muted));
            }
            RunState::Idle => {}
        }
        if let Some(message) = &self.notice {
            body = body.child(note(message.clone(), bt.colors.warn).id("suggest-notice"));
        }
        if has_result && items.is_empty() {
            body = body.child(note(t!("ai.suggest.none").to_string(), bt.colors.muted));
        }
        for (ix, item) in items.iter().enumerate() {
            body = body.child(self.row(ix, item, cx));
        }
        div()
            .id("context-suggestions")
            .child(
                Card::new()
                    .ai()
                    .child(v_flex().gap(m.space[4]).child(header).child(body)),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::ui::testing::{TestAppContext, gpui_test};
    use bitacora_runtime::ai::recommend::Suggestions;

    fn outcome() -> RecommendOutcome {
        RecommendOutcome {
            suggestions: Suggestions {
                related_pages: vec![RelatedPage {
                    page: "Kubernetes".into(),
                    reason: "same topic".into(),
                }],
                link_suggestions: vec![LinkSuggestion {
                    block_uuid: "u1".into(),
                    text: "rust".into(),
                    target: "Rust".into(),
                    start: 6,
                    end: 10,
                }],
                tag_suggestions: vec![TagSuggestion {
                    tag: "ai".into(),
                    reason: String::new(),
                }],
                next_actions: vec!["Write the summary".into()],
            },
            dropped_links: 0,
            dropped_other: 0,
        }
    }

    fn setup(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            crate::theme::install(cx, crate::settings::AppSettings::default(), None);
        });
    }

    fn on(cx: &mut crate::ui::testing::VisualTestContext, v: &crate::ui::Entity<SuggestionsView>) {
        v.update(cx, |v, cx| {
            v.set_gate(
                AiGate {
                    enabled: true,
                    connected: true,
                },
                cx,
            );
            v.set_page(Some("Rust".into()), cx);
        });
    }

    #[gpui_test]
    fn results_belong_to_the_page_and_dismissals_stick(cx: &mut TestAppContext) {
        setup(cx);
        let dir = tempfile::tempdir().unwrap();
        let (v, cx) = cx.add_window_view(|_, _| SuggestionsView::new());
        on(cx, &v);
        v.update(cx, |v, cx| {
            v.set_dismiss_store(DismissStore::load(dir.path(), "/g"), cx);
            v.finish("Rust", Ok(outcome()), cx);
            assert_eq!(v.items().len(), 4);
            let tag = v
                .items()
                .into_iter()
                .find(|i| matches!(i, Item::Tag(_)))
                .unwrap();
            v.dismiss(&tag, cx);
            assert_eq!(v.items().len(), 3);
            // Another page drops the suggestions of this one.
            v.set_page(Some("Go".into()), cx);
            assert!(v.items().is_empty());
            v.set_page(Some("Rust".into()), cx);
            assert!(v.items().is_empty());
            v.finish("Rust", Ok(outcome()), cx);
            assert_eq!(v.items().len(), 3, "the dismissed tag is not offered again");
        });
        // A new session on this machine still remembers.
        let (w, cx2) = cx.add_window_view(|_, _| SuggestionsView::new());
        on(cx2, &w);
        w.update(cx2, |w, cx| {
            w.set_dismiss_store(DismissStore::load(dir.path(), "/g"), cx);
            w.finish("Rust", Ok(outcome()), cx);
            assert_eq!(w.items().len(), 3);
        });
    }

    #[gpui_test]
    fn accepting_removes_the_chip_and_a_failure_keeps_it(cx: &mut TestAppContext) {
        setup(cx);
        let (v, cx) = cx.add_window_view(|_, _| SuggestionsView::new());
        on(cx, &v);
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&v, move |_, e: &SuggestEvent, _| {
                sink.borrow_mut().push(e.clone());
            })
            .detach();
        });
        v.update(cx, |v, cx| {
            v.finish("Rust", Ok(outcome()), cx);
            let link = v
                .items()
                .into_iter()
                .find(|i| matches!(i, Item::Link(_)))
                .unwrap();
            let key = link.key("Rust");
            v.accepted(key.clone(), "Rust".into(), Err("stale".into()), cx);
            assert_eq!(v.notice(), Some("stale"));
            assert!(v.items().contains(&link));
            v.accepted(key, "Rust".into(), Ok(()), cx);
            assert!(!v.items().contains(&link));
        });
        assert_eq!(
            events.borrow().last(),
            Some(&SuggestEvent::Edited("Rust".into()))
        );
    }

    const UUID: &str = "11111111-1111-4111-8111-111111111111";

    fn text_of(queue: &bitacora_core::queue::CommandQueue, page: &str) -> String {
        queue
            .snapshot(&PageKey::from_title(page))
            .unwrap()
            .blocks
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn accepted_link_and_tag_edit_the_page_through_the_queue_and_undo() {
        use bitacora_core::queue::Request;
        use bitacora_runtime::{RuntimeConfig, Session};

        let graph = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let root = graph.path();
        std::fs::create_dir_all(root.join("logseq")).unwrap();
        std::fs::create_dir_all(root.join("pages")).unwrap();
        std::fs::write(root.join("logseq/config.edn"), "{}").unwrap();
        let original = format!("tags:: lang\n\n- Learn rust today\n  id:: {UUID}\n");
        std::fs::write(root.join("pages/Notes.md"), &original).unwrap();
        let mut cfg = RuntimeConfig::new(root);
        cfg.data_dir = Some(data.path().to_path_buf());
        cfg.global_config = Some(data.path().join("no-global.edn"));
        cfg.watch = None;
        cfg.debounce = None;
        let session = Session::open(cfg).unwrap();
        let handle = crate::data::GraphHandle {
            reader: session.read_api(),
            root: session.root().to_path_buf(),
            settings: Arc::new(crate::data::ViewSettings::from_config(session.config())),
        };
        let queue = session.queue().clone();
        let resolver = AppResolver::new(queue.clone(), handle, Arc::new(session.config().clone()));
        let link = Item::Link(LinkSuggestion {
            block_uuid: UUID.into(),
            text: "rust".into(),
            target: "Rust".into(),
            start: 6,
            end: 10,
        });
        let tag = Item::Tag(TagSuggestion {
            tag: "ai".into(),
            reason: String::new(),
        });

        perform(
            &link,
            "Notes",
            Some(session.agent_edit_applier()),
            &resolver,
            &queue,
        )
        .unwrap();
        assert!(text_of(&queue, "Notes").contains("Learn [[Rust]] today"));
        perform(&tag, "Notes", None, &resolver, &queue).unwrap();
        let snap = queue.snapshot(&PageKey::from_title("Notes")).unwrap();
        assert!(
            snap.preamble
                .as_deref()
                .unwrap()
                .contains("tags:: lang, ai")
        );
        // The same tag again is a no-op, and a link whose span moved is refused.
        perform(&tag, "Notes", None, &resolver, &queue).unwrap();
        let stale = perform(
            &link,
            "Notes",
            Some(session.agent_edit_applier()),
            &resolver,
            &queue,
        );
        assert!(stale.is_err(), "the block no longer holds the quoted text");

        // Both are ordinary transactions: two undos restore the page.
        for _ in 0..2 {
            queue.execute(Source::Ui, Request::Undo).unwrap();
        }
        assert!(text_of(&queue, "Notes").contains("Learn rust today"));
        let snap = queue.snapshot(&PageKey::from_title("Notes")).unwrap();
        assert!(!snap.preamble.as_deref().unwrap().contains("ai"));
        queue.execute(Source::Ui, Request::Flush).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("pages/Notes.md")).unwrap(),
            original,
            "undoing both edits gives back the exact bytes"
        );
        let _ = session.shutdown(std::time::Duration::from_secs(10));
    }

    #[gpui_test]
    fn switching_the_feature_off_removes_the_chips(cx: &mut TestAppContext) {
        setup(cx);
        let (v, cx) = cx.add_window_view(|_, _| SuggestionsView::new());
        v.update(cx, |v, _| assert!(!v.visible()));
        on(cx, &v);
        v.update(cx, |v, cx| {
            assert!(v.visible());
            v.finish("Rust", Ok(outcome()), cx);
            v.set_gate(AiGate::default(), cx);
            assert!(!v.visible() && v.items().is_empty());
            // Not connected: asking fails with a reason instead of running.
            v.set_gate(
                AiGate {
                    enabled: true,
                    connected: false,
                },
                cx,
            );
            v.request(cx);
            assert!(matches!(v.run_state(), RunState::Failed(_)));
        });
        cx.run_until_parked();
    }
}
