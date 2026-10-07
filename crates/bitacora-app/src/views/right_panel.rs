//! The right panel (BIT-US-0125): a fixed-width column with a `Segmented` switch between the
//! **Context** tab and the **Agent** tab.
//!
//! * Context shows what surrounds the page on screen: its local graph (BIT-US-0159), page
//!   properties, backlinks and linked references, followed by the stack of pages and blocks opened
//!   with Shift+click (the [`RightSidebar`] stack, which keeps owning that state).
//! * Agent is a shell for the assistant chat of the Pando epic. Until a chat view is mounted with
//!   [`RightPanel::set_agent_slot`] it shows an empty state, or "Pando not configured" while
//!   [`RightPanel::set_agent_configured`] has not been called with `true`.

use bitacora_index::RefFilters;
use rust_i18n::t;

use crate::data::{self, GraphHandle};
use crate::render::inline::NavTarget;
use crate::render::model::PropertyRow;
use crate::ui::theme::{ActiveBitacoraTheme as _, TypeStyleExt as _};
use crate::ui::{
    ActiveTheme as _, AnyElement, AnyView, AppContext as _, Context, Entity,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, div, h_flex, px,
    v_flex,
};
use crate::views::block_view::properties_table;
use crate::views::graph_view::{GraphMode, GraphView};
use crate::views::kit::{Card, Glyph, Overline, Segmented, glyph};
use crate::views::page_view::PageEvent;
use crate::views::right_sidebar::RightSidebar;

/// Height of the local graph widget.
const LOCAL_GRAPH_HEIGHT: f32 = 240.0;
/// Linked-reference snippets listed per referencing page.
const SNIPPETS_PER_PAGE: usize = 3;
/// Longest snippet, in characters.
const SNIPPET_CHARS: usize = 140;

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backlink {
    /// Title of the referencing page.
    pub page: String,
    /// Number of referencing blocks on it.
    pub count: usize,
    /// First lines of some of those blocks.
    pub snippets: Vec<String>,
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
            snippets: g
                .blocks
                .iter()
                .take(SNIPPETS_PER_PAGE)
                .map(|hit| snippet(&hit.block.title, &hit.block.content))
                .collect(),
        })
        .collect();
    Ok(info)
}

/// A one-line preview of a block.
fn snippet(title: &str, content: &str) -> String {
    let text = if title.trim().is_empty() {
        content.lines().next().unwrap_or_default()
    } else {
        title
    };
    let text = text.trim();
    if text.chars().count() > SNIPPET_CHARS {
        let cut: String = text.chars().take(SNIPPET_CHARS).collect();
        format!("{cut}...")
    } else {
        text.to_owned()
    }
}

/// The panel view.
pub struct RightPanel {
    stack: Entity<RightSidebar>,
    tab: PanelTab,
    handle: Option<GraphHandle>,
    local: Entity<GraphView>,
    local_page: Option<String>,
    context: ContextInfo,
    context_task: Option<Task<()>>,
    generation: u64,
    agent_configured: bool,
    agent_slot: Option<AnyView>,
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
            local,
            local_page: None,
            context: ContextInfo::default(),
            context_task: None,
            generation: 0,
            agent_configured: false,
            agent_slot: None,
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
            self.local.update(cx, |v, cx| v.set_page(page, cx));
            self.reload_context(cx);
            cx.notify();
        }
    }

    /// Connects the panel (and the stack) to an open graph.
    pub fn set_graph(&mut self, handle: GraphHandle, cx: &mut Context<Self>) {
        self.handle = Some(handle.clone());
        self.local.update(cx, |v, cx| v.show(handle.clone(), cx));
        self.stack.update(cx, |s, cx| s.set_graph(handle, cx));
        self.reload_context(cx);
    }

    /// Forgets the graph and empties the stack.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.handle = None;
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

    fn context_tab(&mut self, cx: &mut Context<Self>) -> AnyElement {
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
            col = col.child(self.backlinks_section(cx));
        }
        col.child(self.stack.clone()).into_any_element()
    }

    fn backlinks_section(&mut self, cx: &mut Context<Self>) -> AnyElement {
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
        for (ix, link) in self.context.backlinks.iter().enumerate() {
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
            for line in &link.snippets {
                card = card.child(
                    div()
                        .pl(m.space[6])
                        .truncate()
                        .text_color(bt.colors.muted)
                        .type_style(&bt.type_scale.ui_small)
                        .child(SharedString::from(line.clone())),
                );
            }
            section = section.child(div().id(("backlink", ix)).child(card));
        }
        section.into_any_element()
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bt = cx.bitacora().clone();
        let body = match self.tab {
            PanelTab::Context => self.context_tab(cx),
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

    #[test]
    fn snippets_are_one_trimmed_line() {
        assert_eq!(snippet("  hello  ", "ignored"), "hello");
        assert_eq!(snippet("", "first\nsecond"), "first");
        let long = "x".repeat(SNIPPET_CHARS + 10);
        assert_eq!(snippet(&long, "").chars().count(), SNIPPET_CHARS + 3);
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
        assert!(info.backlinks[0].snippets[0].contains("Alpha"));
        // A page the index does not know has neither.
        let none = load_context(&g.handle, "Nowhere").expect("context");
        assert!(none.properties.is_empty() && none.backlinks.is_empty());
    }
}
