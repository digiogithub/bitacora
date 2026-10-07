//! The center of the main window: navigation between the journals feed and page views, with
//! back/forward history (BIT-US-0075, BIT-US-0076).
//!
//! `MainView` owns the [`PageView`] and the [`JournalsView`], turns clicks on refs, tags and
//! breadcrumbs into [`Route`]s and remembers where each visited route was scrolled to.

use bitacora_index::IndexEvent;

use crate::data::GraphHandle;
use crate::nav::{NavHistory, Route, Scroll};
use crate::render::inline::NavTarget;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::{
    ActiveTheme as _, App, AppContext as _, Context, Disableable as _, Entity, EventEmitter,
    IconName, IntoElement, ParentElement as _, Render, Sizable as _, Styled as _, Subscription,
    Window, div, h_flex, px, v_flex,
};
use crate::views::all_pages::AllPagesView;
use crate::views::graph_view::{GraphMode, GraphView};
use crate::views::journals::JournalsView;
use crate::views::page_view::{BlockFocusEvent, ConflictJumpEvent, PageEvent, PageView};
use crate::views::tasks::TasksView;

/// What a pane tells its host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainEvent {
    /// The pane now shows this route (recent pages, sidebar highlight).
    Visited(Route),
    /// Show this route in the right sidebar (Shift+click).
    OpenInSidebar(Route),
    /// A block of the page got (or lost) the focus (BIT-T-0344).
    BlockFocus(BlockFocusEvent),
    /// A conflict-marked block was clicked: open the resolver.
    ConflictJump,
    /// Rename a page (BIT-T-0157).
    RenamePage {
        /// Current title.
        from: String,
        /// New title.
        to: String,
    },
    /// Delete the file behind an asset link, after asking (BIT-US-0096).
    DeleteAsset {
        /// Link target.
        link: String,
        /// Index uuid of the block that holds the link.
        block: Option<String>,
    },
}

/// Journals feed, all-pages table and page view with history: one pane of the main area. Every
/// pane (dock panel) owns its own `MainView`, so splitting the main area never shares state.
pub struct MainView {
    page: Entity<PageView>,
    journals: Entity<JournalsView>,
    all_pages: Entity<AllPagesView>,
    graph: Entity<GraphView>,
    tasks: Entity<TasksView>,
    handle: Option<GraphHandle>,
    history: NavHistory,
    pending: Option<Route>,
    _subscriptions: Vec<Subscription>,
}

impl std::fmt::Debug for MainView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MainView")
            .field("route", &self.route())
            .finish_non_exhaustive()
    }
}

impl EventEmitter<MainEvent> for MainView {}

impl MainView {
    /// Creates the views; nothing is shown until [`MainView::set_graph`].
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let page = cx.new(PageView::new);
        let journals = cx.new(JournalsView::new);
        let all_pages = cx.new(|cx| AllPagesView::new(window, cx));
        let graph = cx.new(|_| GraphView::new(GraphMode::Global));
        let tasks = cx.new(TasksView::new);
        let subscriptions = vec![
            cx.subscribe(&page, Self::on_page_event),
            cx.subscribe(&page, |_, _, event: &BlockFocusEvent, cx| {
                cx.emit(MainEvent::BlockFocus(event.clone()));
            }),
            cx.subscribe(&page, |_, _, _: &ConflictJumpEvent, cx| {
                cx.emit(MainEvent::ConflictJump);
            }),
            cx.subscribe(&journals, Self::on_page_event),
            cx.subscribe(&all_pages, Self::on_page_event),
            cx.subscribe(&graph, Self::on_page_event),
            cx.subscribe(&tasks, Self::on_page_event),
        ];
        Self {
            page,
            journals,
            all_pages,
            graph,
            tasks,
            handle: None,
            history: NavHistory::new(),
            pending: None,
            _subscriptions: subscriptions,
        }
    }

    /// The page view entity.
    pub fn page(&self) -> &Entity<PageView> {
        &self.page
    }

    /// The journals view entity.
    pub fn journals(&self) -> &Entity<JournalsView> {
        &self.journals
    }

    /// The all-pages view entity.
    pub fn all_pages(&self) -> &Entity<AllPagesView> {
        &self.all_pages
    }

    /// The graph view entity.
    pub fn graph(&self) -> &Entity<GraphView> {
        &self.graph
    }

    /// Index id of the page on screen (the "this page" search scope).
    pub fn current_page_id(&self, cx: &App) -> Option<i64> {
        match self.route()? {
            Route::Page(_) => self.page.read(cx).header().page_id,
            Route::Journals | Route::AllPages | Route::Graph | Route::Tasks | Route::Block(_) => {
                None
            }
        }
    }

    /// The route being shown.
    pub fn route(&self) -> Option<&Route> {
        self.history.current().map(|e| &e.route)
    }

    /// Whether back has somewhere to go.
    pub fn can_back(&self) -> bool {
        self.history.can_back()
    }

    /// Whether forward has somewhere to go.
    pub fn can_forward(&self) -> bool {
        self.history.can_forward()
    }

    /// Connects the views to an open graph and shows `initial` (the journals when `None`).
    pub fn set_graph(
        &mut self,
        handle: GraphHandle,
        initial: Option<Route>,
        cx: &mut Context<Self>,
    ) {
        self.handle = Some(handle);
        self.history = NavHistory::new();
        let route = initial.or(self.pending.take()).unwrap_or(Route::Journals);
        self.navigate(route, cx);
        self.journals.update(cx, |j, cx| j.start_clock(cx));
    }

    /// Forgets the graph (it was closed).
    pub fn clear_graph(&mut self, cx: &mut Context<Self>) {
        self.handle = None;
        self.history = NavHistory::new();
        self.all_pages.update(cx, |v, cx| v.clear(cx));
        self.graph.update(cx, |v, cx| v.clear(cx));
        self.tasks.update(cx, |v, cx| v.clear(cx));
        cx.notify();
    }

    fn current_scroll(&self, cx: &Context<Self>) -> Option<Scroll> {
        Some(match self.route()? {
            Route::Journals => self.journals.read(cx).scroll(),
            Route::AllPages | Route::Graph | Route::Tasks => Scroll::default(),
            Route::Page(_) | Route::Block(_) => self.page.read(cx).scroll(),
        })
    }

    /// Opens `route`, pushing the current location on the back stack.
    pub fn navigate(&mut self, route: Route, cx: &mut Context<Self>) {
        let Some(handle) = self.handle.clone() else {
            self.pending = Some(route);
            return;
        };
        if let Some(scroll) = self.current_scroll(cx) {
            self.history.set_scroll(scroll);
        }
        self.history.visit(route.clone());
        cx.emit(MainEvent::Visited(route.clone()));
        self.display(handle, route, None, cx);
    }

    fn display(
        &mut self,
        handle: GraphHandle,
        route: Route,
        restore: Option<Scroll>,
        cx: &mut Context<Self>,
    ) {
        match route {
            Route::Journals => self
                .journals
                .update(cx, |j, cx| j.show(handle, restore, cx)),
            Route::AllPages => self.all_pages.update(cx, |v, cx| v.show(handle, cx)),
            Route::Graph => self.graph.update(cx, |v, cx| v.show(handle, cx)),
            Route::Tasks => self.tasks.update(cx, |v, cx| v.show(handle, cx)),
            Route::Page(_) | Route::Block(_) => {
                self.page
                    .update(cx, |p, cx| p.show(handle, route, restore, cx));
            }
        }
        cx.notify();
    }

    /// Goes back to the previous location (Mod+[).
    pub fn go_back(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.handle.clone() else {
            return;
        };
        if let Some(scroll) = self.current_scroll(cx) {
            self.history.set_scroll(scroll);
        }
        if let Some(entry) = self.history.back() {
            cx.emit(MainEvent::Visited(entry.route.clone()));
            self.display(handle, entry.route, Some(entry.scroll), cx);
        }
    }

    /// Goes forward again (Mod+]).
    pub fn go_forward(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.handle.clone() else {
            return;
        };
        if let Some(scroll) = self.current_scroll(cx) {
            self.history.set_scroll(scroll);
        }
        if let Some(entry) = self.history.forward() {
            cx.emit(MainEvent::Visited(entry.route.clone()));
            self.display(handle, entry.route, Some(entry.scroll), cx);
        }
    }

    /// Connects the page view to the live session so pages become editable.
    pub fn set_session_link(
        &mut self,
        link: crate::session::SessionLink,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tasks
            .update(cx, |t, cx| t.set_session_link(Some(link.clone()), cx));
        self.graph
            .update(cx, |g, _| g.set_queue(Some(link.queue.clone())));
        self.journals
            .update(cx, |j, cx| j.set_session_link(Some(link.clone()), cx));
        self.page
            .update(cx, |p, cx| p.set_session_link(Some(link), window, cx));
    }

    /// The block being edited changed on disk.
    pub fn on_editing_conflict(
        &mut self,
        conflict: &bitacora_core::editor::EditingConflict,
        cx: &mut Context<Self>,
    ) {
        self.page
            .update(cx, |p, cx| p.on_editing_conflict(conflict, cx));
        self.journals
            .update(cx, |j, cx| j.on_editing_conflict(conflict, cx));
    }

    /// Reloads what is shown (the startup index reconcile finished).
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        match self.route() {
            Some(Route::Journals) => self.journals.update(cx, |j, cx| j.refresh(cx)),
            Some(Route::AllPages) => self.all_pages.update(cx, |v, cx| v.reload(cx)),
            Some(Route::Graph) => self.graph.update(cx, |v, cx| v.reload(cx)),
            Some(Route::Tasks) => self.tasks.update(cx, |v, cx| v.reload(cx)),
            Some(_) => self.page.update(cx, |p, cx| p.reload(cx)),
            None => {}
        }
    }

    /// Forwards an index change to the view on screen.
    pub fn on_index_event(&mut self, event: &IndexEvent, cx: &mut Context<Self>) {
        match self.route() {
            Some(Route::Journals) => self
                .journals
                .update(cx, |j, cx| j.on_index_event(event, cx)),
            Some(Route::AllPages) => self.all_pages.update(cx, |v, cx| v.on_index_changed(cx)),
            Some(Route::Graph) => self.graph.update(cx, |v, cx| v.on_index_event(event, cx)),
            Some(Route::Tasks) => self.tasks.update(cx, |v, cx| v.on_index_changed(cx)),
            Some(_) => self.page.update(cx, |p, cx| p.on_index_event(event, cx)),
            None => {}
        }
    }

    fn on_page_event(
        &mut self,
        _: Entity<impl Sized + 'static>,
        event: &PageEvent,
        cx: &mut Context<Self>,
    ) {
        let (PageEvent::Navigate(target) | PageEvent::OpenInSidebar(target)) = event else {
            match event {
                PageEvent::DeleteAsset { link, block } => cx.emit(MainEvent::DeleteAsset {
                    link: link.clone(),
                    block: block.clone(),
                }),
                PageEvent::RenamePage { from, to } => cx.emit(MainEvent::RenamePage {
                    from: from.clone(),
                    to: to.clone(),
                }),
                _ => {}
            }
            return;
        };
        let sidebar = matches!(event, PageEvent::OpenInSidebar(_));
        let route = match target {
            NavTarget::Url(url) => {
                cx.open_url(url);
                return;
            }
            NavTarget::Page(name) => Route::Page(name.clone()),
            NavTarget::Block(uuid) => Route::Block(uuid.clone()),
        };
        if sidebar {
            cx.emit(MainEvent::OpenInSidebar(route));
        } else {
            self.navigate(route, cx);
        }
    }
}

impl Render for MainView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let this = cx.entity();
        let back = this.clone();
        let forward = this;
        let shown = match self.route() {
            Some(Route::Journals) => self.journals.clone().into_any_element(),
            Some(Route::AllPages) => self.all_pages.clone().into_any_element(),
            Some(Route::Graph) => self.graph.clone().into_any_element(),
            Some(Route::Tasks) => self.tasks.clone().into_any_element(),
            _ => self.page.clone().into_any_element(),
        };
        v_flex()
            .size_full()
            .bg(theme.background)
            .child(
                h_flex()
                    .px(px(8.))
                    .py(px(4.))
                    .gap_1()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        Button::new("nav-back")
                            .ghost()
                            .small()
                            .icon(IconName::ArrowLeft)
                            .disabled(!self.can_back())
                            .on_click(move |_, _, cx| back.update(cx, |m, cx| m.go_back(cx))),
                    )
                    .child(
                        Button::new("nav-forward")
                            .ghost()
                            .small()
                            .icon(IconName::ArrowRight)
                            .disabled(!self.can_forward())
                            .on_click(move |_, _, cx| forward.update(cx, |m, cx| m.go_forward(cx))),
                    ),
            )
            .child(div().flex_1().min_h_0().child(shown))
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

    fn open(cx: &mut TestAppContext) -> (Entity<MainView>, &mut VisualTestContext) {
        cx.add_window_view(MainView::new)
    }

    fn settle(
        cx: &mut VisualTestContext,
        view: &Entity<MainView>,
        done: impl Fn(&PageView) -> bool,
    ) {
        cx.executor().allow_parking();
        let page = view.read_with(cx, |m, _| m.page().clone());
        for _ in 0..400 {
            cx.run_until_parked();
            if page.read_with(cx, |p, _| done(p)) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("main view did not settle");
    }

    fn long() -> String {
        (0..300).map(|n| format!("- row {n}\n")).collect()
    }

    #[gpui_test]
    fn back_and_forward_walk_the_history_and_restore_scroll(cx: &mut TestAppContext) {
        setup(cx);
        let long = long();
        let g = TestGraph::new(&[
            ("pages/A.md", &long),
            ("pages/B.md", "- see [[A]]\n"),
            ("pages/C.md", "- c\n"),
        ]);
        let (main, cx) = open(cx);
        main.update(cx, |m, cx| {
            m.set_graph(g.handle.clone(), Some(Route::Page("A".into())), cx);
        });
        settle(cx, &main, |p| *p.state() == LoadState::Loaded);
        let page = main.read_with(cx, |m, _| m.page().clone());
        // Scroll down in A, then follow a link to B.
        page.update(cx, |p, _| {
            p.restore_scroll(Scroll {
                item_ix: 20,
                offset_px: 4.,
            });
        });
        let before = page.read_with(cx, |p, _| p.scroll());
        assert!(before.item_ix >= 19, "scrolled down: {before:?}");
        page.update(cx, |_, cx| {
            cx.emit(PageEvent::Navigate(NavTarget::Page("B".into())));
        });
        settle(cx, &main, |p| p.header().title == "B");
        assert_eq!(
            main.read_with(cx, |m, _| m.route().cloned()),
            Some(Route::Page("B".into()))
        );
        assert!(main.read_with(cx, |m, _| m.can_back()));
        main.update(cx, |m, cx| m.navigate(Route::Page("C".into()), cx));
        settle(cx, &main, |p| p.header().title == "C");
        main.update(cx, |m, cx| m.go_back(cx));
        settle(cx, &main, |p| p.header().title == "B");
        main.update(cx, |m, cx| m.go_back(cx));
        settle(cx, &main, |p| p.header().title == "A");
        assert!(!main.read_with(cx, |m, _| m.can_back()));
        // Scroll position of A came back.
        let scroll = page.read_with(cx, |p, _| p.scroll());
        // The list may normalize the position by one item once it is laid out.
        assert!(
            scroll.item_ix.abs_diff(before.item_ix) <= 1,
            "scroll restored: {scroll:?} vs {before:?}"
        );
        main.update(cx, |m, cx| m.go_forward(cx));
        settle(cx, &main, |p| p.header().title == "B");
        assert!(main.read_with(cx, |m, _| m.can_forward()));
    }

    #[gpui_test]
    fn journals_route_and_pages_share_one_history(cx: &mut TestAppContext) {
        setup(cx);
        let g = TestGraph::new(&[("journals/2024_01_01.md", "- j\n"), ("pages/A.md", "- a\n")]);
        let (main, cx) = open(cx);
        main.update(cx, |m, cx| m.set_graph(g.handle.clone(), None, cx));
        assert_eq!(
            main.read_with(cx, |m, _| m.route().cloned()),
            Some(Route::Journals)
        );
        main.update(cx, |m, cx| m.navigate(Route::Page("A".into()), cx));
        settle(cx, &main, |p| p.header().title == "A");
        main.update(cx, |m, cx| m.go_back(cx));
        assert_eq!(
            main.read_with(cx, |m, _| m.route().cloned()),
            Some(Route::Journals)
        );
    }

    #[gpui_test]
    fn navigation_requested_before_the_graph_is_ready_is_kept(cx: &mut TestAppContext) {
        setup(cx);
        let g = TestGraph::new(&[("pages/A.md", "- a\n")]);
        let (main, cx) = open(cx);
        main.update(cx, |m, cx| m.navigate(Route::Page("A".into()), cx));
        assert!(main.read_with(cx, |m, _| m.route().is_none()));
        main.update(cx, |m, cx| m.set_graph(g.handle.clone(), None, cx));
        settle(cx, &main, |p| p.header().title == "A");
    }
}
