//! Content of the title bar (BIT-US-0122): sidebar toggle, back/forward, page tabs, the
//! search field that opens the palette, theme / PDF / assistant / right-panel buttons and, on
//! Linux and Windows, the in-window "Graph" menu that stands in for the missing native menu bar.
//!
//! Every button calls the same handler as its action, so shortcuts and menus keep working.

use crate::views::dims;
use rust_i18n::t;

use super::Workspace;
use crate::actions::OpenSearch;
use crate::nav::Route;
use crate::ui::button::{Button as KitButton, ButtonVariants as _};
use crate::ui::dock::DockPlacement;
use crate::ui::frameless::MouseButton;
use crate::ui::theme::ActiveBitacoraTheme as _;
use crate::ui::{
    Anchor, AnyElement, Context, FluentBuilder as _, InteractiveElement as _, IntoElement,
    ParentElement as _, Sizable as _, StatefulInteractiveElement as _, Styled as _, anchored,
    deferred, div, h_flex, v_flex,
};
use crate::views::kit::{Button, Glyph, IconButton, Kbd, PopoverShell, Tab, glyph};
use crate::views::responsive::Breakpoint;
use crate::views::title_bar::AppTitleBar;

/// Whether the bar hosts the in-window application menu: everywhere but macOS, where the
/// native menu bar carries the same entries.
pub fn has_in_window_menu(macos: bool) -> bool {
    !macos
}

/// Hint shown in the search field for the shortcut bound to `OpenSearch`.
fn search_hint(macos: bool) -> &'static str {
    if macos { "\u{2318}K" } else { "Ctrl K" }
}

/// The open page tabs: each tab remembers the route it shows; the active one follows
/// navigation, the others wait to be selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabStrip {
    tabs: Vec<Route>,
    active: usize,
}

impl Default for TabStrip {
    fn default() -> Self {
        Self::new()
    }
}

impl TabStrip {
    /// One tab on the journals feed.
    pub fn new() -> Self {
        Self {
            tabs: vec![Route::Journals],
            active: 0,
        }
    }

    /// The routes of the tabs, left to right.
    pub fn routes(&self) -> &[Route] {
        &self.tabs
    }

    /// Index of the active tab.
    pub fn active(&self) -> usize {
        self.active
    }

    /// The pane navigated to `route`: the active tab now shows it.
    pub fn visit(&mut self, route: Route) {
        self.tabs[self.active] = route;
    }

    /// Opens a new active tab on `route`.
    pub fn open(&mut self, route: Route) {
        self.tabs.push(route);
        self.active = self.tabs.len() - 1;
    }

    /// Activates tab `ix`; returns its route when the active tab changed.
    pub fn activate(&mut self, ix: usize) -> Option<Route> {
        if ix >= self.tabs.len() || ix == self.active {
            return None;
        }
        self.active = ix;
        Some(self.tabs[ix].clone())
    }

    /// Closes tab `ix` (the last tab never closes). Returns the route to show when the closed
    /// tab was the active one.
    pub fn close(&mut self, ix: usize) -> Option<Route> {
        if self.tabs.len() <= 1 || ix >= self.tabs.len() {
            return None;
        }
        self.tabs.remove(ix);
        if ix < self.active {
            self.active -= 1;
            None
        } else if ix == self.active {
            self.active = ix.min(self.tabs.len() - 1);
            Some(self.tabs[self.active].clone())
        } else {
            None
        }
    }
}

/// Title and icon of the tab that shows `route`.
pub fn tab_label(route: &Route) -> (String, Glyph) {
    match route {
        Route::Journals => (t!("sidebar.journals").to_string(), Glyph::Calendar),
        Route::AllPages => (t!("sidebar.all_pages").to_string(), Glyph::Database),
        Route::Page(name) => (name.clone(), Glyph::File),
        Route::Block(_) => (t!("top_bar.block_tab").to_string(), Glyph::File),
        Route::Graph => (t!("sidebar.graph_view").to_string(), Glyph::Network),
        Route::Tasks => (t!("tasks.title").to_string(), Glyph::SquareCheck),
    }
}

/// Swallows the press so a click on a bar control never starts a window drag.
fn no_drag(el: impl IntoElement) -> AnyElement {
    no_drag_inner(el)
}

fn no_drag_inner(el: impl IntoElement) -> AnyElement {
    div()
        .flex_shrink_0()
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .child(el)
        .into_any_element()
}

impl Workspace {
    /// Moves the tab strip along with the pane (called from the `Visited` handler).
    pub(super) fn tabs_visit(&mut self, route: &Route) {
        self.tabs.visit(route.clone());
    }

    fn tab_activate(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(route) = self.tabs.activate(ix) {
            self.navigate(route, cx);
        }
        cx.notify();
    }

    fn tab_close(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(route) = self.tabs.close(ix) {
            self.navigate(route, cx);
        }
        cx.notify();
    }

    /// Opens `route` in a new tab (Ctrl/Cmd+click on a page or block reference). The tab the
    /// click came from keeps its route.
    pub fn open_in_new_tab(&mut self, route: Route, cx: &mut Context<Self>) {
        self.tabs.open(route.clone());
        self.navigate(route, cx);
        cx.notify();
    }

    /// The tab strip (tests and diagnostics).
    pub fn tab_strip(&self) -> &TabStrip {
        &self.tabs
    }

    fn tab_new(&mut self, cx: &mut Context<Self>) {
        self.tabs.open(Route::Journals);
        self.navigate(Route::Journals, cx);
        cx.notify();
    }

    fn set_app_menu(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.app_menu_open != open {
            self.app_menu_open = open;
            cx.notify();
        }
    }

    /// The title bar with its slots filled.
    pub(super) fn title_bar(
        &mut self,
        breakpoint: Breakpoint,
        cx: &mut Context<Self>,
    ) -> AppTitleBar {
        let narrow = breakpoint == Breakpoint::Narrow;
        let macos = cfg!(target_os = "macos");
        let theme = cx.bitacora().clone();
        let colors = theme.colors;
        let metrics = theme.metrics.clone();
        let sidebar_visible = self.sidebar.read(cx).is_visible();
        let right_open = self.dock.read(cx).is_dock_open(DockPlacement::Right);
        let (can_back, can_forward) = {
            let main = self.main.read(cx);
            (main.can_back(), main.can_forward())
        };
        let in_graph = !self.picker_visible;
        let dark = theme.mode == crate::ui::theme::Mode::Dark;

        let mut bar = AppTitleBar::new();

        if has_in_window_menu(macos) {
            bar = bar.left(self.app_menu(cx));
        }

        bar = bar
            .left(no_drag(
                IconButton::new("top-sidebar", Glyph::PanelLeft)
                    .small()
                    .active(sidebar_visible)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sidebar.update(cx, |s, cx| s.toggle(cx));
                    })),
            ))
            .left(no_drag(
                IconButton::new("top-back", Glyph::ChevronLeft)
                    .small()
                    .disabled(!(in_graph && can_back))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.main.update(cx, |m, cx| m.go_back(cx));
                    })),
            ))
            .left(no_drag(
                IconButton::new("top-forward", Glyph::ChevronRight)
                    .small()
                    .disabled(!(in_graph && can_forward))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.main.update(cx, |m, cx| m.go_forward(cx));
                    })),
            ));

        // Tabs sit on the bottom edge of the bar so the active one melts into the content.
        let active = self.tabs.active();
        let many = self.tabs.routes().len() > 1;
        let tabs = self
            .tabs
            .routes()
            .iter()
            .enumerate()
            .map(|(ix, route)| {
                let (title, icon) = tab_label(route);
                Tab::new(("top-tab", ix), title)
                    .icon(icon)
                    .active(ix == active)
                    .on_click(cx.listener(move |this, _, _, cx| this.tab_activate(ix, cx)))
                    .when(many, |t| {
                        t.on_close(cx.listener(move |this, _, _, cx| this.tab_close(ix, cx)))
                    })
            })
            .collect::<Vec<_>>();
        bar = bar.left(
            h_flex()
                .debug_selector(|| "title-tabs".to_string())
                .h_full()
                .min_w_0()
                .overflow_hidden()
                .items_end()
                .gap(metrics.space[2])
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .children(tabs)
                .when(in_graph, |d| {
                    d.child(
                        div().h(metrics.tab_height).flex().items_center().child(
                            IconButton::new("top-new-tab", Glyph::Plus)
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| this.tab_new(cx))),
                        ),
                    )
                }),
        );

        // Search: opens the palette like the shortcut does. Medium widths shrink the field
        // (it keeps a minimum and clips its label); Narrow collapses it to an icon button.
        let open_search = cx.listener(|this, _, window, cx| {
            this.open_search(&OpenSearch, window, cx);
        });
        if narrow {
            bar = bar.center_min_width(metrics.icon_button_sm).center(no_drag(
                IconButton::new("top-search", Glyph::Search)
                    .small()
                    .on_click(open_search),
            ));
        } else {
            let search = h_flex()
                .id("top-search")
                .debug_selector(|| "top-search".to_string())
                .flex_1()
                .min_w_0()
                .max_w(dims::PX_300)
                .h(metrics.icon_button_sm)
                .px(metrics.space[5])
                .gap(metrics.space[4])
                .items_center()
                .overflow_hidden()
                .bg(colors.bg)
                .border_1()
                .border_color(colors.line)
                .rounded(metrics.radius_control)
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_click(open_search)
                .child(glyph(Glyph::Search, metrics.icon_sm, colors.muted, cx))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(colors.muted)
                        .child(t!("top_bar.search").to_string()),
                )
                .child(Kbd::new(search_hint(macos)));
            bar = bar
                .center_min_width(dims::PX_150)
                .center(div().flex_1().min_w_0().max_w(dims::PX_300).child(search));
        }

        bar.right(no_drag(
            h_flex()
                .gap(metrics.space[2])
                // Theme and PDF are the least important: dropped at Narrow widths (the theme
                // stays reachable from settings and the command palette).
                .when(!narrow, |d| {
                    d.child(
                        IconButton::new("top-theme", if dark { Glyph::Sun } else { Glyph::Moon })
                            .on_click(cx.listener(|_, _, window, cx| {
                                crate::theme::toggle(cx, Some(window));
                            })),
                    )
                    // PDF export has no backend yet: shown disabled until it lands.
                    .child(IconButton::new("top-pdf", Glyph::Printer).disabled(true))
                })
                // Connection status and quick settings of Pando (BIT-T-0429).
                .child(no_drag_inner(self.pando_control(cx)))
                // The assistant lives in the right panel's Agent tab.
                .child(
                    IconButton::new("top-ai", Glyph::Sparkle).on_click(
                        cx.listener(|this, _, window, cx| this.open_agent_panel(window, cx)),
                    ),
                )
                .child(
                    IconButton::new("top-right-panel", Glyph::PanelRight)
                        .active(right_open)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.dock.update(cx, |area, cx| {
                                area.toggle_dock(DockPlacement::Right, window, cx)
                            });
                        })),
                )
                // Settings gear with the full app menu (BIT-US-0175), last before the window
                // controls.
                .child(self.gear_menu(macos, cx)),
        ))
    }

    /// The gear button and its popover: "Settings...", every settings section and the
    /// application menu entries (BIT-US-0175).
    fn gear_menu(&mut self, macos: bool, cx: &mut Context<Self>) -> AnyElement {
        use crate::menus::{GearCommand, GearRow};
        let open = self.gear_menu_open;
        let theme = cx.bitacora().clone();
        let colors = theme.colors;
        let metrics = theme.metrics.clone();
        let tip = if macos {
            t!("top_bar.settings_tip_mac")
        } else {
            t!("top_bar.settings_tip")
        }
        .to_string();
        let button = KitButton::new("top-settings")
            .ghost()
            .small()
            .icon(crate::ui::IconName::Settings)
            .tooltip(tip)
            .on_click(cx.listener(|this, _, _, cx| {
                let open = !this.gear_menu_open;
                this.set_gear_menu(open, cx);
            }));

        let popover = open.then(|| {
            let mut body = v_flex().gap(metrics.space[1]);
            for (ix, row) in crate::menus::gear_rows(self.recents.graphs())
                .into_iter()
                .enumerate()
            {
                match row {
                    GearRow::Separator => {
                        body = body.child(div().h(dims::PX_1).my(metrics.space[2]).bg(colors.line));
                    }
                    GearRow::Heading(text) => {
                        body = body.child(
                            div()
                                .px(metrics.space[4])
                                .py(metrics.space[2])
                                .text_color(colors.muted)
                                .child(text),
                        );
                    }
                    GearRow::Item {
                        label,
                        command,
                        nested,
                    } => {
                        body = body.child(
                            div().when(nested, |d| d.pl(metrics.space[4])).child(
                                Button::new(("gear-item", ix))
                                    .ghost()
                                    .label(label)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.set_gear_menu(false, cx);
                                        match command {
                                            GearCommand::OpenSettings(section) => {
                                                this.open_settings(section, window, cx)
                                            }
                                            GearCommand::OpenGraph => this.open_graph_dialog(cx),
                                            GearCommand::OpenRecent(n) => {
                                                this.open_recent(n, window, cx)
                                            }
                                            GearCommand::CloseGraph => this.close_graph(window, cx),
                                            GearCommand::Quit => cx.quit(),
                                        }
                                    })),
                            ),
                        );
                    }
                }
            }
            let weak = cx.entity().downgrade();
            deferred(
                anchored().anchor(Anchor::TopRight).snap_to_window().child(
                    div().mt(metrics.space[2]).child(
                        PopoverShell::new("gear-menu")
                            .width(dims::PX_240)
                            .on_dismiss(move |_, cx| {
                                let _ = weak.update(cx, |this, cx| this.set_gear_menu(false, cx));
                            })
                            .child(body),
                    ),
                ),
            )
            .priority(10)
        });

        div()
            .relative()
            .child(button)
            .when_some(popover, |d, p| d.child(p))
            .into_any_element()
    }

    fn set_gear_menu(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.gear_menu_open != open {
            self.gear_menu_open = open;
            cx.notify();
        }
    }

    /// The "Graph" button and its popover (Open graph, Open recent, Close graph).
    fn app_menu(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let open = self.app_menu_open;
        let recents = self.recents.graphs().to_vec();
        let theme = cx.bitacora().clone();
        let metrics = theme.metrics.clone();
        let button = Button::new("top-graph-menu")
            .ghost()
            .label(t!("menu.graph").to_string())
            .icon(Glyph::ChevronDown)
            .on_click(cx.listener(|this, _, _, cx| {
                let open = !this.app_menu_open;
                this.set_app_menu(open, cx);
            }));

        let popover = open.then(|| {
            let mut body = v_flex().gap(metrics.space[1]).child(
                Button::new("menu-open-graph")
                    .ghost()
                    .label(t!("menu.open_graph").to_string())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_app_menu(false, cx);
                        this.open_graph_dialog(cx);
                    })),
            );
            body = body.child(
                div()
                    .px(metrics.space[4])
                    .py(metrics.space[2])
                    .text_color(theme.colors.muted)
                    .child(t!("menu.open_recent").to_string()),
            );
            if recents.is_empty() {
                body = body.child(
                    Button::new("menu-no-recent")
                        .ghost()
                        .label(t!("picker.no_recent").to_string())
                        .disabled(true),
                );
            }
            for (ix, graph) in recents.iter().enumerate() {
                body = body.child(
                    Button::new(("menu-recent", ix))
                        .ghost()
                        .label(graph.name())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.set_app_menu(false, cx);
                            this.open_recent(ix, window, cx);
                        })),
                );
            }
            body = body.child(
                Button::new("menu-close-graph")
                    .ghost()
                    .label(t!("menu.close_graph").to_string())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_app_menu(false, cx);
                        this.close_graph(window, cx);
                    })),
            );
            let weak = cx.entity().downgrade();
            deferred(
                anchored().anchor(Anchor::TopLeft).snap_to_window().child(
                    div().mt(metrics.space[2]).child(
                        PopoverShell::new("app-menu")
                            .width(dims::PX_240)
                            .on_dismiss(move |_, cx| {
                                let _ = weak.update(cx, |this, cx| this.set_app_menu(false, cx));
                            })
                            .child(body),
                    ),
                ),
            )
            .priority(10)
        });

        no_drag(
            div()
                .relative()
                .child(button)
                .when_some(popover, |d, p| d.child(p)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(routes: &[&str]) -> TabStrip {
        TabStrip {
            tabs: routes.iter().map(|r| Route::Page((*r).into())).collect(),
            active: 0,
        }
    }

    fn page(n: &str) -> Route {
        Route::Page(n.into())
    }

    #[test]
    fn menu_is_in_window_except_on_macos() {
        assert!(has_in_window_menu(false));
        assert!(!has_in_window_menu(true));
    }

    #[test]
    fn visit_replaces_the_active_tab_only() {
        let mut s = strip(&["a", "b"]);
        s.visit(page("z"));
        assert_eq!(s.routes(), [page("z"), page("b")]);
    }

    #[test]
    fn open_activates_the_new_tab_and_activate_reports_changes() {
        let mut s = strip(&["a"]);
        s.open(Route::Journals);
        assert_eq!(s.active(), 1);
        assert_eq!(s.activate(0), Some(page("a")));
        assert_eq!(s.activate(0), None);
        assert_eq!(s.activate(9), None);
    }

    #[test]
    fn closing_keeps_one_tab_and_picks_a_neighbour() {
        let mut s = strip(&["a", "b", "c"]);
        s.activate(1);
        assert_eq!(s.close(1), Some(page("c")));
        assert_eq!(s.active(), 1);
        assert_eq!(s.close(0), None);
        assert_eq!(s.active(), 0);
        assert_eq!(s.close(0), None, "the last tab stays");
        assert_eq!(s.routes().len(), 1);
    }
}
