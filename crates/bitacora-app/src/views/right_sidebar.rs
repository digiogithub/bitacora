//! The right sidebar stack (BIT-US-0080): pages and block subtrees opened with Shift+click, each
//! in a collapsible, closable card with its own [`PageView`] that refreshes on index events.
//!
//! The stack is the content of the right dock panel; the [`Workspace`](super::workspace) owns it
//! and persists [`RightSidebar::entries`] per graph.

use rust_i18n::t;

use crate::actions::{
    SidebarCloseItem, SidebarOpenItem, SidebarSelectNext, SidebarSelectPrevious, SidebarToggleItem,
};
use crate::data::GraphHandle;
use crate::graph_state::{StackEntry, StoredRoute};
use crate::nav::Route;
use crate::render::inline::NavTarget;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::{
    ActiveTheme as _, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    IconName, InteractiveElement as _, IntoElement, ParentElement as _, Render, Sizable as _,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, h_flex, px, v_flex,
};
use crate::views::graph_view::{GraphMode, GraphView};
use crate::views::page_view::{PageEvent, PageView};

/// Height of an expanded item body.
const ITEM_HEIGHT: f32 = 340.0;
/// Height of the local graph widget.
const LOCAL_GRAPH_HEIGHT: f32 = 240.0;

/// What the stack asks of its host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackEvent {
    /// A link inside an item was clicked: show it in the main area.
    Navigate(NavTarget),
    /// An item was promoted to the main area.
    OpenInMain(Route),
    /// The set of items, their order or their folded state changed (persist it).
    Changed,
}

struct Item {
    id: u64,
    route: Route,
    collapsed: bool,
    view: Entity<PageView>,
    _subscription: Subscription,
}

/// The stack view.
pub struct RightSidebar {
    items: Vec<Item>,
    next_id: u64,
    handle: Option<GraphHandle>,
    selected: Option<usize>,
    focus: FocusHandle,
    /// Local graph of the page on screen (BIT-US-0159); a stand-in for the Context tab.
    local: Entity<GraphView>,
    local_page: Option<String>,
    _local_subscription: Subscription,
}

impl std::fmt::Debug for RightSidebar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RightSidebar")
            .field("items", &self.routes())
            .finish_non_exhaustive()
    }
}

impl EventEmitter<StackEvent> for RightSidebar {}

impl RightSidebar {
    /// An empty stack.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let local = cx.new(|_| GraphView::new(GraphMode::Local));
        let local_subscription =
            cx.subscribe(&local, |this, _, event: &PageEvent, cx| match event {
                PageEvent::Navigate(target) => cx.emit(StackEvent::Navigate(target.clone())),
                PageEvent::OpenInSidebar(target) => this.open_target(target, cx),
                PageEvent::DeleteAsset { .. } | PageEvent::RenamePage { .. } => {}
            });
        Self {
            local,
            local_page: None,
            _local_subscription: local_subscription,
            items: Vec::new(),
            next_id: 1,
            handle: None,
            selected: None,
            focus: cx.focus_handle(),
        }
    }

    /// The local graph widget.
    pub fn local_graph(&self) -> &Entity<GraphView> {
        &self.local
    }

    /// The page whose local graph is shown (`None` hides the widget).
    pub fn set_local_page(&mut self, page: Option<String>, cx: &mut Context<Self>) {
        if self.local_page != page {
            self.local_page = page.clone();
            self.local.update(cx, |v, cx| v.set_page(page, cx));
            cx.notify();
        }
    }

    /// The routes shown, top item first.
    pub fn routes(&self) -> Vec<Route> {
        self.items.iter().map(|i| i.route.clone()).collect()
    }

    /// Number of items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Whether the item at `ix` is folded.
    pub fn is_collapsed(&self, ix: usize) -> bool {
        self.items.get(ix).is_some_and(|i| i.collapsed)
    }

    /// The selected item (keyboard focus within the stack).
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// The page view of the item at `ix`.
    pub fn view(&self, ix: usize) -> Option<&Entity<PageView>> {
        self.items.get(ix).map(|i| &i.view)
    }

    /// The persisted form of the stack.
    pub fn entries(&self) -> Vec<StackEntry> {
        self.items
            .iter()
            .filter_map(|i| {
                Some(StackEntry {
                    route: StoredRoute::from_route(&i.route)?,
                    collapsed: i.collapsed,
                })
            })
            .collect()
    }

    /// Connects every item to an open graph.
    pub fn set_graph(&mut self, handle: GraphHandle, cx: &mut Context<Self>) {
        self.handle = Some(handle.clone());
        self.local.update(cx, |v, cx| v.show(handle.clone(), cx));
        for item in &self.items {
            let route = item.route.clone();
            let handle = handle.clone();
            item.view
                .update(cx, |view, cx| view.show(handle, route, None, cx));
        }
        cx.notify();
    }

    /// Forgets the graph and empties the stack (a different graph was opened).
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.handle = None;
        self.local.update(cx, |v, cx| v.clear(cx));
        self.items.clear();
        self.selected = None;
        cx.notify();
    }

    /// Replaces the stack with `entries` (restored state); items load once a graph is set.
    pub fn restore(&mut self, entries: &[StackEntry], cx: &mut Context<Self>) {
        self.items.clear();
        self.selected = None;
        for entry in entries.iter().rev() {
            let id = self.add_item(entry.route.to_route(), cx);
            if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
                item.collapsed = entry.collapsed;
            }
        }
        // `add_item` puts the newest on top, so restoring in reverse keeps the stored order.
        if let Some(handle) = self.handle.clone() {
            self.set_graph(handle, cx);
        }
        cx.notify();
    }

    fn add_item(&mut self, route: Route, cx: &mut Context<Self>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let view = cx.new(PageView::new);
        let subscription = cx.subscribe(&view, |this, _, event: &PageEvent, cx| match event {
            PageEvent::Navigate(target) => cx.emit(StackEvent::Navigate(target.clone())),
            PageEvent::OpenInSidebar(target) => this.open_target(target, cx),
            // Sidebar pages are read-only: nothing to delete from here.
            PageEvent::DeleteAsset { .. } | PageEvent::RenamePage { .. } => {}
        });
        if let Some(handle) = self.handle.clone() {
            let route = route.clone();
            view.update(cx, |v, cx| v.show(handle, route, None, cx));
        }
        self.items.insert(
            0,
            Item {
                id,
                route,
                collapsed: false,
                view,
                _subscription: subscription,
            },
        );
        id
    }

    fn open_target(&mut self, target: &NavTarget, cx: &mut Context<Self>) {
        match target {
            NavTarget::Page(name) => self.open(Route::Page(name.clone()), cx),
            NavTarget::Block(uuid) => self.open(Route::Block(uuid.clone()), cx),
            NavTarget::Url(url) => cx.open_url(url),
        }
    }

    /// Shows `route` at the top of the stack; an item already showing it moves up and unfolds.
    pub fn open(&mut self, route: Route, cx: &mut Context<Self>) {
        if matches!(route, Route::Journals | Route::AllPages | Route::Graph) {
            return;
        }
        if let Some(ix) = self.items.iter().position(|i| i.route == route) {
            let mut item = self.items.remove(ix);
            item.collapsed = false;
            self.items.insert(0, item);
        } else {
            self.add_item(route, cx);
        }
        self.selected = Some(0);
        cx.emit(StackEvent::Changed);
        cx.notify();
    }

    /// Closes the item at `ix`.
    pub fn close(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.items.len() {
            return;
        }
        self.items.remove(ix);
        self.selected = match self.selected {
            _ if self.items.is_empty() => None,
            Some(s) if s >= self.items.len() => Some(self.items.len() - 1),
            other => other,
        };
        cx.emit(StackEvent::Changed);
        cx.notify();
    }

    /// Folds or unfolds the item at `ix`.
    pub fn toggle(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(item) = self.items.get_mut(ix) {
            item.collapsed = !item.collapsed;
            cx.emit(StackEvent::Changed);
            cx.notify();
        }
    }

    /// Forwards an index change to the items on screen.
    pub fn on_index_event(&mut self, event: &bitacora_index::IndexEvent, cx: &mut Context<Self>) {
        if self.local_page.is_some() {
            self.local.update(cx, |v, cx| v.on_index_event(event, cx));
        }
        for item in self.items.iter().filter(|i| !i.collapsed) {
            item.view.update(cx, |v, cx| v.on_index_event(event, cx));
        }
    }

    /// Reloads every item (the startup reconcile finished).
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        for item in self.items.iter().filter(|i| !i.collapsed) {
            item.view.update(cx, |v, cx| v.reload(cx));
        }
    }

    /// Moves the selection down (to the first item when nothing is selected).
    pub fn select_next(&mut self, _: &SidebarSelectNext, _: &mut Window, cx: &mut Context<Self>) {
        if self.items.is_empty() {
            return;
        }
        self.selected = Some(
            self.selected
                .map_or(0, |s| (s + 1).min(self.items.len() - 1)),
        );
        cx.notify();
    }

    /// Moves the selection up.
    pub fn select_previous(
        &mut self,
        _: &SidebarSelectPrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.items.is_empty() {
            return;
        }
        self.selected = Some(self.selected.map_or(0, |s| s.saturating_sub(1)));
        cx.notify();
    }

    fn toggle_selected(&mut self, _: &SidebarToggleItem, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.selected {
            self.toggle(ix, cx);
        }
    }

    fn close_selected(&mut self, _: &SidebarCloseItem, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.selected {
            self.close(ix, cx);
        }
    }

    fn open_selected(&mut self, _: &SidebarOpenItem, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(item) = self.selected.and_then(|ix| self.items.get(ix)) {
            cx.emit(StackEvent::OpenInMain(item.route.clone()));
        }
    }

    /// Moves the keyboard focus into the stack, selecting the top item.
    pub fn focus_stack(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_none() && !self.items.is_empty() {
            self.selected = Some(0);
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn title(&self, item: &Item, cx: &App) -> String {
        match &item.route {
            Route::Page(name) => name.clone(),
            Route::Block(_) => item
                .view
                .read(cx)
                .header()
                .zoom
                .first()
                .map_or_else(|| t!("right.block").to_string(), |l| l.label.clone()),
            Route::Journals | Route::AllPages | Route::Graph => String::new(),
        }
    }
}

impl Focusable for RightSidebar {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for RightSidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let this = cx.entity();
        let focused = self.focus.contains_focused(window, cx);
        let mut stack = v_flex()
            .id("right-sidebar")
            .key_context("RightSidebar")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::toggle_selected))
            .on_action(cx.listener(Self::close_selected))
            .on_action(cx.listener(Self::open_selected))
            .size_full()
            .overflow_y_scroll()
            .gap_2()
            .p(px(8.));
        if self.local_page.is_some() {
            stack = stack.child(
                v_flex()
                    .id("local-graph")
                    .w_full()
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(6.))
                    .child(
                        div()
                            .px(px(10.))
                            .py(px(6.))
                            .text_sm()
                            .child(t!("graph_view.local_title").to_string()),
                    )
                    .child(
                        div()
                            .h(px(LOCAL_GRAPH_HEIGHT))
                            .border_t_1()
                            .border_color(theme.border)
                            .child(self.local.clone()),
                    ),
            );
        }
        if self.items.is_empty() {
            return stack
                .child(
                    div()
                        .p(px(8.))
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(t!("right.empty_hint").to_string()),
                )
                .into_any_element();
        }
        for (ix, item) in self.items.iter().enumerate() {
            let selected = self.selected == Some(ix) && focused;
            let toggle = this.clone();
            let close = this.clone();
            let header = h_flex()
                .items_center()
                .gap_1()
                .px(px(6.))
                .py(px(4.))
                .child(
                    Button::new(("stack-toggle", ix))
                        .ghost()
                        .small()
                        .icon(if item.collapsed {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        })
                        .on_click(move |_, _, cx| toggle.update(cx, |s, cx| s.toggle(ix, cx))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_sm()
                        .child(self.title(item, cx)),
                )
                .child(
                    Button::new(("stack-close", ix))
                        .ghost()
                        .small()
                        .icon(IconName::Close)
                        .on_click(move |_, _, cx| close.update(cx, |s, cx| s.close(ix, cx))),
                );
            let mut card = v_flex()
                .id(("stack-item", ix))
                .w_full()
                .border_1()
                .border_color(if selected { theme.ring } else { theme.border })
                .rounded(px(6.))
                .child(header);
            if !item.collapsed {
                card = card.child(
                    div()
                        .h(px(ITEM_HEIGHT))
                        .border_t_1()
                        .border_color(theme.border)
                        .child(item.view.clone()),
                );
            }
            stack = stack.child(card);
        }
        stack.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestGraph;
    use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
    use crate::views::page_view::LoadState;
    use crate::{settings::AppSettings, theme};

    fn setup(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            theme::install(cx, AppSettings::default(), None);
        });
    }

    fn open(cx: &mut TestAppContext) -> (Entity<RightSidebar>, &mut VisualTestContext) {
        cx.add_window_view(|_, cx| RightSidebar::new(cx))
    }

    fn page(name: &str) -> Route {
        Route::Page(name.into())
    }

    fn settle_all(cx: &mut VisualTestContext, stack: &Entity<RightSidebar>) {
        cx.executor().allow_parking();
        for _ in 0..400 {
            cx.run_until_parked();
            let n = stack.read_with(cx, |s, _| s.len());
            let ready = (0..n).all(|ix| {
                stack.read_with(cx, |s, cx| {
                    s.view(ix)
                        .is_some_and(|v| *v.read(cx).state() == LoadState::Loaded)
                })
            });
            if ready {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("sidebar items did not load");
    }

    #[gpui_test]
    fn items_stack_dedupe_fold_and_close(cx: &mut TestAppContext) {
        setup(cx);
        let g = TestGraph::new(&[("pages/A.md", "- a\n"), ("pages/B.md", "- b [[A]]\n")]);
        let (stack, cx) = open(cx);
        stack.update(cx, |s, cx| s.set_graph(g.handle.clone(), cx));
        stack.update(cx, |s, cx| {
            s.open(page("A"), cx);
            s.open(page("B"), cx);
        });
        assert_eq!(
            stack.read_with(cx, |s, _| s.routes()),
            vec![page("B"), page("A")]
        );
        settle_all(cx, &stack);
        // Re-opening A moves it to the top and unfolds it.
        stack.update(cx, |s, cx| {
            s.toggle(1, cx);
            assert!(s.is_collapsed(1));
            s.open(page("A"), cx);
        });
        assert_eq!(
            stack.read_with(cx, |s, _| s.routes()),
            vec![page("A"), page("B")]
        );
        assert!(!stack.read_with(cx, |s, _| s.is_collapsed(0)));
        stack.update(cx, |s, cx| s.close(0, cx));
        assert_eq!(stack.read_with(cx, |s, _| s.routes()), vec![page("B")]);
        // The journals feed is not stackable.
        stack.update(cx, |s, cx| s.open(Route::Journals, cx));
        assert_eq!(stack.read_with(cx, |s, _| s.len()), 1);
    }

    #[gpui_test]
    fn stack_round_trips_through_entries(cx: &mut TestAppContext) {
        setup(cx);
        let g = TestGraph::new(&[("pages/A.md", "- a\n"), ("pages/B.md", "- b\n")]);
        let (stack, cx) = open(cx);
        stack.update(cx, |s, cx| {
            s.open(page("A"), cx);
            s.open(page("B"), cx);
            s.toggle(1, cx);
        });
        let entries = stack.read_with(cx, |s, _| s.entries());
        assert_eq!(entries.len(), 2);
        let (second, cx) = open(cx);
        second.update(cx, |s, cx| {
            s.restore(&entries, cx);
            s.set_graph(g.handle.clone(), cx);
        });
        assert_eq!(
            second.read_with(cx, |s, _| s.routes()),
            vec![page("B"), page("A")]
        );
        assert!(second.read_with(cx, |s, _| s.is_collapsed(1)));
        settle_all(cx, &second);
    }

    #[gpui_test]
    fn keyboard_selection_toggles_and_closes(cx: &mut TestAppContext) {
        setup(cx);
        let (stack, cx) = open(cx);
        stack.update(cx, |s, cx| {
            s.open(page("A"), cx);
            s.open(page("B"), cx);
        });
        stack.update_in(cx, |s, window, cx| s.focus_stack(window, cx));
        cx.dispatch_action(SidebarSelectNext);
        assert_eq!(stack.read_with(cx, |s, _| s.selected()), Some(1));
        cx.dispatch_action(SidebarSelectPrevious);
        assert_eq!(stack.read_with(cx, |s, _| s.selected()), Some(0));
        stack.update_in(cx, |s, window, cx| s.focus_stack(window, cx));
        cx.dispatch_action(SidebarToggleItem);
        assert!(stack.read_with(cx, |s, _| s.is_collapsed(0)));
        cx.dispatch_action(SidebarCloseItem);
        assert_eq!(stack.read_with(cx, |s, _| s.routes()), vec![page("A")]);
    }
}
