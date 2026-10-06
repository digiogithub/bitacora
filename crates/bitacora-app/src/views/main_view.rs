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
    ActiveTheme as _, AppContext as _, Context, Disableable as _, Entity, IconName, IntoElement,
    ParentElement as _, Render, Sizable as _, Styled as _, Subscription, Window, div, h_flex, px,
    v_flex,
};
use crate::views::journals::JournalsView;
use crate::views::page_view::{PageEvent, PageView};

/// Journals feed plus page view with history.
pub struct MainView {
    page: Entity<PageView>,
    journals: Entity<JournalsView>,
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

impl MainView {
    /// Creates the views; nothing is shown until [`MainView::set_graph`].
    pub fn new(cx: &mut Context<Self>) -> Self {
        let page = cx.new(PageView::new);
        let journals = cx.new(JournalsView::new);
        let subscriptions = vec![
            cx.subscribe(&page, Self::on_page_event),
            cx.subscribe(&journals, Self::on_page_event),
        ];
        Self {
            page,
            journals,
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
        cx.notify();
    }

    fn current_scroll(&self, cx: &Context<Self>) -> Option<Scroll> {
        Some(match self.route()? {
            Route::Journals => self.journals.read(cx).scroll(),
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
            self.display(handle, entry.route, Some(entry.scroll), cx);
        }
    }

    /// Reloads what is shown (the startup index reconcile finished).
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        match self.route() {
            Some(Route::Journals) => self.journals.update(cx, |j, cx| j.refresh(cx)),
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
        let PageEvent::Navigate(target) = event;
        match target {
            NavTarget::Url(url) => cx.open_url(url),
            NavTarget::Page(name) => self.navigate(Route::Page(name.clone()), cx),
            NavTarget::Block(uuid) => self.navigate(Route::Block(uuid.clone()), cx),
        }
    }
}

impl Render for MainView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let this = cx.entity();
        let back = this.clone();
        let forward = this;
        let showing_journals = matches!(self.route(), Some(Route::Journals));
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
            .child(div().flex_1().min_h_0().child(if showing_journals {
                self.journals.clone().into_any_element()
            } else {
                self.page.clone().into_any_element()
            }))
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
        cx.add_window_view(|_, cx| MainView::new(cx))
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
