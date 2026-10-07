//! The right panel (BIT-US-0125): a fixed-width column with a `Segmented` switch between the
//! **Context** tab and the **Agent** tab.
//!
//! * Context shows what surrounds the page on screen: its local graph (BIT-US-0159), page
//!   properties, backlinks and linked references, followed by the stack of pages and blocks opened
//!   with Shift+click (the [`RightSidebar`] stack, which keeps owning that state).
//! * Agent is a shell for the assistant chat of the Pando epic. Until a chat view is mounted with
//!   [`RightPanel::set_agent_slot`] it shows an empty state, or "Pando not configured" while
//!   [`RightPanel::set_agent_configured`] has not been called with `true`.

use std::rc::Rc;

use bitacora_index::RefFilters;
use rust_i18n::t;

use crate::data::{self, GraphHandle};
use crate::nav::OpenIn;
use crate::render::inline::NavTarget;
use crate::render::model::{PropertyRow, Row};
use crate::session::SessionLink;
use crate::ui::theme::{ActiveBitacoraTheme as _, TypeStyleExt as _};
use crate::ui::{
    ActiveTheme as _, AnyElement, AnyView, AppContext as _, Context, Entity, FluentBuilder as _,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, div, h_flex, px,
    v_flex,
};
use crate::views::ai_assist::suggestions::SuggestionsView;
use crate::views::block_view::{Nav, RowActions, properties_table, render_block_row};
use crate::views::graph_view::{GraphMode, GraphView};
use crate::views::kit::{Card, Glyph, Overline, Segmented, glyph};
use crate::views::page_view::PageEvent;
use crate::views::related::{self, Related};
use crate::views::remote_edit::RemoteEditors;
use crate::views::right_sidebar::RightSidebar;

/// Height of the local graph widget.
const LOCAL_GRAPH_HEIGHT: f32 = 240.0;
/// Linked-reference blocks listed per referencing page.
const BLOCKS_PER_PAGE: usize = 3;
/// Base of the element ids of the backlink rows (they must not collide with other rows).
const BACKLINK_ROW_ID: usize = 1 << 34;

/// The two tabs of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelTab {
    /// Local graph, properties, references and the opened-items stack.
    #[default]
    Context,
    /// The assistant chat.
    Agent,
}

impl PanelTab {
    const fn key(self) -> &'static str {
        match self {
            Self::Context => "context",
            Self::Agent => "agent",
        }
    }

    fn from_key(key: &str) -> Self {
        if key == "agent" {
            Self::Agent
        } else {
            Self::Context
        }
    }
}

/// One referencing page in the Context tab.
#[derive(Debug, Clone, PartialEq)]
pub struct Backlink {
    /// Title of the referencing page.
    pub page: String,
    /// Number of referencing blocks on it.
    pub count: usize,
    /// Some of those blocks, drawn as rows: a click on one edits it in its page.
    pub blocks: Vec<Row>,
}

/// What the Context tab shows about the page on screen.
#[derive(Debug, Clone, Default)]
pub struct ContextInfo {
    /// Page properties (hidden built-ins removed).
    pub properties: Vec<PropertyRow>,
    /// Pages that reference it.
    pub backlinks: Vec<Backlink>,
}

/// Loads [`ContextInfo`] of page `name` from the index.
pub fn load_context(handle: &GraphHandle, name: &str) -> Result<ContextInfo, String> {
    let load = data::open_page(handle, name, 1)?;
    let mut info = ContextInfo {
        properties: load.header.properties,
        backlinks: Vec::new(),
    };
    let Some(page_id) = load.header.page_id else {
        return Ok(info);
    };
    let groups = handle
        .reader
        .linked_references_with(page_id, &RefFilters::default())
        .map_err(|e| e.to_string())?;
    info.backlinks = groups
        .into_iter()
        .map(|g| Backlink {
            page: g.page.original_name,
            count: g.blocks.len(),
            blocks: g
                .blocks
                .iter()
                .take(BLOCKS_PER_PAGE)
                .flat_map(|hit| {
                    let mut rows = data::rows_from_blocks(
                        handle,
                        std::slice::from_ref(&hit.block),
                        hit.block.depth - 1,
                    );
                    // Only the block itself: its children are not part of the backlink.
                    for row in &mut rows {
                        row.has_children = false;
                        row.depth = 0;
                    }
                    rows
                })
                .collect(),
        })
        .collect();
    Ok(info)
}

/// The panel view.
pub struct RightPanel {
    stack: Entity<RightSidebar>,
    tab: PanelTab,
    handle: Option<GraphHandle>,
    link: Option<SessionLink>,
    local: Entity<GraphView>,
    local_page: Option<String>,
    context: ContextInfo,
    context_task: Option<Task<()>>,
    generation: u64,
    hybrid: Option<bitacora_runtime::HybridSearch>,
    related: Related,
    related_task: Option<Task<()>>,
    related_generation: u64,
    agent_configured: bool,
    agent_slot: Option<AnyView>,
    /// AI suggestion chips of the Context tab (BIT-US-0152).
    suggestions: Option<Entity<SuggestionsView>>,
    /// Editors of the source pages of the backlink blocks (click to edit in place).
    remote: RemoteEditors<RightPanel>,
    _subscriptions: Vec<Subscription>,
}

impl std::fmt::Debug for RightPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RightPanel")
            .field("tab", &self.tab)
            .field("local_page", &self.local_page)
            .finish_non_exhaustive()
    }
}

impl RightPanel {
    /// A panel around the opened-items `stack`, on the Context tab.
    pub fn new(stack: Entity<RightSidebar>, cx: &mut Context<Self>) -> Self {
        let local = cx.new(|_| GraphView::new(GraphMode::Local));
        let subscription = cx.subscribe(&local, |this, _, event: &PageEvent, cx| match event {
            PageEvent::Navigate(target) => this
                .stack
                .update(cx, |stack, cx| stack.navigate(target.clone(), cx)),
            PageEvent::OpenInSidebar(target) => this
                .stack
                .update(cx, |stack, cx| stack.open_in_stack(target, cx)),
            PageEvent::DeleteAsset { .. } | PageEvent::RenamePage { .. } => {}
        });
        Self {
            stack,
            tab: PanelTab::Context,
            handle: None,
            link: None,
            local,
            local_page: None,
            context: ContextInfo::default(),
            context_task: None,
            generation: 0,
            hybrid: None,
            related: Related::Idle,
            related_task: None,
            related_generation: 0,
            agent_configured: false,
            agent_slot: None,
            suggestions: None,
            remote: RemoteEditors::new(|v| &mut v.remote, None),
            _subscriptions: vec![subscription],
        }
    }

    /// The opened-items stack shown under the context cards.
    pub fn stack(&self) -> &Entity<RightSidebar> {
        &self.stack
    }

    /// The local graph widget.
    pub fn local_graph(&self) -> &Entity<GraphView> {
        &self.local
    }

    /// The tab on screen.
    pub fn tab(&self) -> PanelTab {
        self.tab
    }

    /// Switches tab.
    pub fn set_tab(&mut self, tab: PanelTab, cx: &mut Context<Self>) {
        if self.tab != tab {
            self.tab = tab;
            cx.notify();
        }
    }

    /// Mounts the assistant chat view in the Agent tab (`None` goes back to the empty state).
    pub fn set_agent_slot(&mut self, view: Option<AnyView>, cx: &mut Context<Self>) {
        self.agent_slot = view;
        cx.notify();
    }

    /// Whether a chat view is mounted in the Agent tab.
    pub fn has_agent_slot(&self) -> bool {
        self.agent_slot.is_some()
    }

    /// Tells the Agent tab whether Pando is configured (selects empty vs "not configured").
    pub fn set_agent_configured(&mut self, configured: bool, cx: &mut Context<Self>) {
        if self.agent_configured != configured {
            self.agent_configured = configured;
            cx.notify();
        }
    }

    /// The page whose context is shown (`None` hides the page widgets).
    pub fn local_page(&self) -> Option<&str> {
        self.local_page.as_deref()
    }

    /// What the Context tab knows about the page.
    pub fn context(&self) -> &ContextInfo {
        &self.context
    }

    /// Follows the page on screen.
    pub fn set_local_page(&mut self, page: Option<String>, cx: &mut Context<Self>) {
        if self.local_page != page {
            self.local_page = page.clone();
            self.local.update(cx, |v, cx| v.set_page(page.clone(), cx));
            if let Some(chips) = &self.suggestions {
                chips.update(cx, |v, cx| v.set_page(page, cx));
            }
            self.reload_context(cx);
            self.reload_related(cx);
            cx.notify();
        }
    }

    /// Mounts the AI suggestion chips under the related blocks of the Context tab.
    pub fn set_suggestions(
        &mut self,
        view: Option<Entity<SuggestionsView>>,
        cx: &mut Context<Self>,
    ) {
        self.suggestions = view;
        if let Some(chips) = &self.suggestions {
            let page = self.local_page.clone();
            chips.update(cx, |v, cx| v.set_page(page, cx));
        }
        cx.notify();
    }

    /// Connects the panel (and the stack) to an open graph.
    pub fn set_graph(&mut self, handle: GraphHandle, cx: &mut Context<Self>) {
        self.handle = Some(handle.clone());
        self.remote
            .configure(self.link.clone(), Some(handle.clone()));
        self.local.update(cx, |v, cx| v.show(handle.clone(), cx));
        self.stack.update(cx, |s, cx| s.set_graph(handle, cx));
        self.reload_context(cx);
        self.reload_related(cx);
    }

    /// Connects the backlinks to the live session: their blocks can be edited in place.
    pub fn set_session_link(&mut self, link: Option<SessionLink>, cx: &mut Context<Self>) {
        self.link = link.clone();
        self.remote.configure(link, self.handle.clone());
        cx.notify();
    }

    /// The editor of source page `page`, once a click created it (tests).
    #[cfg(test)]
    pub(crate) fn editor(
        &self,
        page: &str,
    ) -> Option<crate::ui::Entity<crate::editor::OutlineEditor>> {
        self.remote.editor(page).cloned()
    }

    /// What a click on backlink block `n` of referencing page `ix` does (tests).
    #[cfg(test)]
    pub(crate) fn click_backlink(
        &mut self,
        (ix, n): (usize, usize),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(link) = self.context.backlinks.get(ix) else {
            return;
        };
        let Some(row) = link.blocks.get(n) else {
            return;
        };
        if let Some(target) = crate::views::remote_edit::BlockRef::of_row(&link.page, row) {
            self.remote.activate(&target, usize::MAX, false, window, cx);
        }
    }

    /// The block being edited changed on disk: the backlink editors look at it.
    pub fn on_editing_conflict(
        &mut self,
        conflict: &bitacora_core::editor::EditingConflict,
        cx: &mut Context<Self>,
    ) {
        self.remote.on_editing_conflict(conflict, cx);
    }

    /// Gives the panel the session's hybrid searcher for the Related blocks section.
    pub fn set_hybrid(
        &mut self,
        hybrid: Option<bitacora_runtime::HybridSearch>,
        cx: &mut Context<Self>,
    ) {
        self.hybrid = hybrid;
        self.reload_related(cx);
    }

    /// The Related blocks section state.
    pub fn related(&self) -> &Related {
        &self.related
    }

    fn reload_related(&mut self, cx: &mut Context<Self>) {
        self.related_generation += 1;
        let generation = self.related_generation;
        self.related_task = None;
        let (Some(handle), Some(hybrid), Some(page)) = (
            self.handle.clone(),
            self.hybrid.clone(),
            self.local_page.clone(),
        ) else {
            self.related = Related::Idle;
            return;
        };
        self.related = Related::Loading;
        self.related_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { related::load_related(&handle, &hybrid, &page) })
                .await;
            let _ = this.update(cx, |panel, cx| {
                if panel.related_generation == generation {
                    panel.related = result.unwrap_or_else(|message| {
                        tracing::warn!("related blocks failed: {message}");
                        Related::Idle
                    });
                    cx.notify();
                }
            });
        }));
    }

    /// Forgets the graph and empties the stack.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.handle = None;
        self.related = Related::Idle;
        self.related_task = None;
        self.related_generation += 1;
        self.local.update(cx, |v, cx| v.clear(cx));
        self.stack.update(cx, |s, cx| s.clear(cx));
        self.context = ContextInfo::default();
        self.context_task = None;
        cx.notify();
    }

    /// Forwards an index change to the widgets on screen.
    pub fn on_index_event(&mut self, event: &bitacora_index::IndexEvent, cx: &mut Context<Self>) {
        if self.local_page.is_some() {
            self.local.update(cx, |v, cx| v.on_index_event(event, cx));
            self.reload_context(cx);
        }
        self.stack.update(cx, |s, cx| s.on_index_event(event, cx));
    }

    fn reload_context(&mut self, cx: &mut Context<Self>) {
        let (Some(handle), Some(page)) = (self.handle.clone(), self.local_page.clone()) else {
            self.context = ContextInfo::default();
            self.context_task = None;
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        self.context_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { load_context(&handle, &page) })
                .await;
            let _ = this.update(cx, |panel, cx| {
                if panel.generation == generation {
                    panel.context = result.unwrap_or_default();
                    cx.notify();
                }
            });
        }));
    }

    fn open_page(&mut self, name: &str, cx: &mut Context<Self>) {
        let target = NavTarget::Page(name.to_owned());
        self.stack.update(cx, |s, cx| s.navigate(target, cx));
    }

    fn tab_switch(&self, cx: &mut Context<Self>) -> Segmented {
        let this = cx.entity();
        Segmented::new("right-panel-tabs")
            .option(PanelTab::Context.key(), t!("right.tab_context").to_string())
            .option(PanelTab::Agent.key(), t!("right.tab_agent").to_string())
            .selected(self.tab.key())
            .on_change(move |key, _, cx| {
                let tab = PanelTab::from_key(key);
                this.update(cx, |panel, cx| panel.set_tab(tab, cx));
            })
    }

    fn context_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let bt = cx.bitacora().clone();
        let m = &bt.metrics;
        let theme = cx.theme().clone();
        let mut col = v_flex()
            .id("right-panel-context")
            .size_full()
            .overflow_y_scroll()
            .gap(m.space[4])
            .p(m.space[4]);
        if self.local_page.is_some() {
            col = col.child(
                Card::new()
                    .padding(px(0.), px(0.))
                    .child(
                        div()
                            .id("local-graph")
                            .px(m.space[4])
                            .py(m.space[3])
                            .child(Overline::new(t!("graph_view.local_title").to_string())),
                    )
                    .child(
                        div()
                            .h(px(LOCAL_GRAPH_HEIGHT))
                            .border_t_1()
                            .border_color(bt.colors.line)
                            .child(self.local.clone()),
                    ),
            );
            if !self.context.properties.is_empty() {
                col = col.child(
                    v_flex()
                        .id("context-properties")
                        .gap(m.space[2])
                        .child(Overline::new(t!("right.properties").to_string()))
                        .child(properties_table(1, &self.context.properties, &theme, None)),
                );
            }
            col = col.child(self.backlinks_section(window, cx));
            if let Some(section) = self.related_section(cx) {
                col = col.child(section);
            }
            if let Some(chips) = &self.suggestions {
                col = col.child(chips.clone());
            }
        }
        col.child(self.stack.clone()).into_any_element()
    }

    fn backlinks_section(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let bt = cx.bitacora().clone();
        let m = &bt.metrics;
        let total: usize = self.context.backlinks.iter().map(|b| b.count).sum();
        let mut section = v_flex()
            .id("context-backlinks")
            .gap(m.space[2])
            .child(Overline::new(t!("right.backlinks").to_string()).count(total));
        if self.context.backlinks.is_empty() {
            return section
                .child(
                    div()
                        .text_color(bt.colors.muted)
                        .type_style(&bt.type_scale.ui_small)
                        .child(t!("right.no_backlinks").to_string()),
                )
                .into_any_element();
        }
        let backlinks = self.context.backlinks.clone();
        for (ix, link) in backlinks.iter().enumerate() {
            let page = link.page.clone();
            let mut card = v_flex().gap(m.space[1]).child(
                h_flex()
                    .id(("backlink-open", ix))
                    .gap(m.space[2])
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| this.open_page(&page, cx)))
                    .child(glyph(Glyph::File, m.icon_sm, bt.colors.muted, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(bt.colors.text)
                            .type_style(&bt.type_scale.ui)
                            .child(link.page.clone()),
                    )
                    .child(
                        div()
                            .text_color(bt.colors.muted)
                            .type_style(&bt.type_scale.mono)
                            .child(link.count.to_string()),
                    ),
            );
            for (n, row) in link.blocks.iter().enumerate() {
                card = card.child(self.backlink_block(ix, n, &link.page, row, window, cx));
            }
            section = section.child(div().id(("backlink", ix)).child(card));
        }
        section.into_any_element()
    }

    /// One backlink block: its text edits in place on click, links inside still navigate.
    fn backlink_block(
        &mut self,
        ix: usize,
        n: usize,
        page: &str,
        row: &Row,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bt = cx.bitacora().clone();
        let theme = cx.theme().clone();
        let this = cx.entity();
        let stack = self.stack.clone();
        let nav: Nav = Rc::new(move |target, open, cx| {
            stack.update(cx, |stack, cx| match open {
                OpenIn::Sidebar => stack.open_in_stack(&target, cx),
                OpenIn::Main => stack.navigate(target, cx),
            });
        });
        let remote = self
            .remote
            .prepare(&this, page, row, None, None, window, cx);
        let editing = remote.as_ref().is_some_and(|rr| rr.editing);
        let (edit, activate, shown) = match remote {
            Some(rr) => (rr.edit, rr.activate, Some(rr.row)),
            None => (None, None, None),
        };
        let actions = RowActions {
            edit,
            activate,
            ..RowActions::nav_only(nav)
        };
        let root = self.handle.as_ref().map(|h| h.root.clone());
        let el = render_block_row(
            BACKLINK_ROW_ID + ix * 64 + n,
            shown.as_ref().unwrap_or(row),
            root.as_deref(),
            &theme,
            &bt,
            &actions,
        );
        let el = if editing {
            self.remote.wrap(page, el, cx)
        } else {
            el
        };
        div().pl(bt.metrics.space[2]).child(el).into_any_element()
    }

    /// The "Related blocks" section; hidden unless semantic search is working or failing.
    fn related_section(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let bt = cx.bitacora().clone();
        let m = &bt.metrics;
        let note = |text: String| {
            div()
                .text_color(bt.colors.muted)
                .type_style(&bt.type_scale.ui_small)
                .child(text)
        };
        let mut section = v_flex().id("context-related").gap(m.space[2]);
        match &self.related {
            Related::Idle => return None,
            Related::Loading => {
                section = section
                    .child(Overline::new(t!("right.related").to_string()))
                    .child(note(t!("right.related_loading").to_string()));
            }
            Related::Unavailable(why) => {
                section = section
                    .child(Overline::new(t!("right.related").to_string()))
                    .child(note(why.clone()));
            }
            Related::Ready(blocks) => {
                section = section
                    .child(Overline::new(t!("right.related").to_string()).count(blocks.len()));
                if blocks.is_empty() {
                    section = section.child(note(related::empty_text()));
                }
                for (ix, block) in blocks.iter().enumerate() {
                    let uuid = block.uuid.clone();
                    section = section.child(
                        v_flex()
                            .id(("related-block", ix))
                            .gap(m.space[1])
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let target = NavTarget::Block(uuid.clone());
                                this.stack.update(cx, |s, cx| s.navigate(target, cx));
                            }))
                            .child(
                                div()
                                    .truncate()
                                    .text_color(bt.colors.text)
                                    .type_style(&bt.type_scale.ui)
                                    .child(SharedString::from(block.text.clone())),
                            )
                            .child(
                                h_flex()
                                    .gap(m.space[2])
                                    .child(glyph(Glyph::File, m.icon_sm, bt.colors.muted, cx))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_color(bt.colors.muted)
                                            .type_style(&bt.type_scale.ui_small)
                                            .child(SharedString::from(block.page.clone())),
                                    )
                                    .when(block.stale, |row| {
                                        row.child(
                                            div()
                                                .text_color(bt.colors.muted)
                                                .type_style(&bt.type_scale.ui_small)
                                                .child(t!("palette.semantic_stale").to_string()),
                                        )
                                    }),
                            ),
                    );
                }
            }
        }
        Some(section.into_any_element())
    }

    fn agent_tab(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(slot) = &self.agent_slot {
            return div()
                .id("right-panel-agent")
                .size_full()
                .child(slot.clone())
                .into_any_element();
        }
        let bt = cx.bitacora().clone();
        let m = &bt.metrics;
        let (title, hint, id) = if self.agent_configured {
            (
                t!("right.agent_empty_title"),
                t!("right.agent_empty_hint"),
                "agent-empty",
            )
        } else {
            (
                t!("right.agent_unconfigured_title"),
                t!("right.agent_unconfigured_hint"),
                "agent-unconfigured",
            )
        };
        div()
            .id("right-panel-agent")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p(m.space[6])
            .child(
                v_flex()
                    .id(id)
                    .items_center()
                    .gap(m.space[3])
                    .text_center()
                    .child(glyph(Glyph::Sparkle, m.icon, bt.colors.ai, cx))
                    .child(
                        div()
                            .text_color(bt.colors.text)
                            .type_style(&bt.type_scale.ui)
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .text_color(bt.colors.muted)
                            .type_style(&bt.type_scale.ui_small)
                            .child(hint.to_string()),
                    ),
            )
            .into_any_element()
    }
}

impl Render for RightPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bt = cx.bitacora().clone();
        let body = match self.tab {
            PanelTab::Context => self.context_tab(window, cx),
            PanelTab::Agent => self.agent_tab(cx),
        };
        v_flex()
            .id("right-panel")
            .key_context("RightPanel")
            .size_full()
            .bg(bt.colors.panel)
            .child(
                h_flex()
                    .flex_none()
                    .p(bt.metrics.space[4])
                    .border_b_1()
                    .border_color(bt.colors.line)
                    .child(self.tab_switch(cx)),
            )
            .child(div().flex_1().min_h_0().child(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestGraph;
    use crate::ui::testing::{TestAppContext, gpui_test};
    use crate::{settings::AppSettings, theme};

    struct Chat;

    impl Render for Chat {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    fn setup(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            theme::install(cx, AppSettings::default(), None);
        });
    }

    #[test]
    fn tab_keys_round_trip() {
        for tab in [PanelTab::Context, PanelTab::Agent] {
            assert_eq!(PanelTab::from_key(tab.key()), tab);
        }
        assert_eq!(PanelTab::from_key("unknown"), PanelTab::Context);
    }

    #[gpui_test]
    fn tabs_agent_states_and_slot(cx: &mut TestAppContext) {
        setup(cx);
        let (panel, cx) = cx.add_window_view(|_, cx| {
            let stack = cx.new(RightSidebar::new);
            RightPanel::new(stack, cx)
        });
        assert_eq!(panel.read_with(cx, |p, _| p.tab()), PanelTab::Context);
        panel.update(cx, |p, cx| p.set_tab(PanelTab::Agent, cx));
        assert_eq!(panel.read_with(cx, |p, _| p.tab()), PanelTab::Agent);
        // Not configured by default; configuring flips to the empty state; both render.
        cx.run_until_parked();
        panel.update(cx, |p, cx| p.set_agent_configured(true, cx));
        cx.run_until_parked();
        // A mounted chat view takes the tab over.
        let chat = cx.update(|_, cx| cx.new(|_| Chat));
        panel.update(cx, |p, cx| p.set_agent_slot(Some(chat.into()), cx));
        assert!(panel.read_with(cx, |p, _| p.has_agent_slot()));
        cx.run_until_parked();
        panel.update(cx, |p, cx| p.set_agent_slot(None, cx));
        assert!(!panel.read_with(cx, |p, _| p.has_agent_slot()));
    }

    #[gpui_test]
    fn context_lists_properties_and_backlinks(cx: &mut TestAppContext) {
        setup(cx);
        let g = TestGraph::new(&[
            ("pages/Alpha.md", "type:: note\n\n- alpha\n"),
            ("pages/Beta.md", "- see [[Alpha]] now\n- also [[Alpha]]\n"),
            ("pages/Gamma.md", "- [[Alpha]]\n"),
        ]);
        let info = load_context(&g.handle, "Alpha").expect("context");
        assert_eq!(info.properties.len(), 1);
        assert_eq!(info.properties[0].key, "type");
        let pages: Vec<_> = info
            .backlinks
            .iter()
            .map(|b| (b.page.as_str(), b.count))
            .collect();
        assert_eq!(pages, [("Beta", 2), ("Gamma", 1)]);
        assert!(
            info.backlinks[0].blocks[0]
                .block
                .title
                .text
                .contains("Alpha")
        );
        // A page the index does not know has neither.
        let none = load_context(&g.handle, "Nowhere").expect("context");
        assert!(none.properties.is_empty() && none.backlinks.is_empty());
    }
}
