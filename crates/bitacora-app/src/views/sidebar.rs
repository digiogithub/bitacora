//! Left sidebar (BIT-US-0079): Journals, All pages, Favorites (`:favorites` of `config.edn`,
//! read-only here), Recent pages of the graph and a graph switcher. The entry of the page on
//! screen is highlighted.

use rust_i18n::t;

use crate::nav::Route;
use crate::ui::sidebar::{
    Sidebar, SidebarCollapsible, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem,
};
use crate::ui::{
    Context, EventEmitter, IconName, IntoElement, ParentElement as _, Render, Styled as _, Window,
};

/// Where a sidebar click wants to go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Journals index.
    Journals,
    /// All pages list.
    AllPages,
    /// A favorite or recent page.
    Page(String),
    /// Graph switcher.
    GraphSwitcher,
}

/// Events emitted by the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarEvent {
    /// The user picked a destination.
    Navigate(Target),
    /// The user Shift+clicked a page: show it in the right sidebar.
    OpenInSidebar(String),
}

/// The left sidebar view.
#[derive(Debug)]
pub struct LeftSidebar {
    graph_name: Option<String>,
    visible: bool,
    active: Option<Target>,
    favorites: Vec<String>,
    recent: Vec<String>,
}

impl EventEmitter<SidebarEvent> for LeftSidebar {}

impl LeftSidebar {
    /// Creates a visible sidebar for the (optional) open graph.
    pub fn new(graph_name: Option<String>) -> Self {
        Self {
            graph_name,
            visible: true,
            active: None,
            favorites: Vec::new(),
            recent: Vec::new(),
        }
    }

    /// Sets the graph name shown in the header.
    pub fn set_graph_name(&mut self, name: Option<String>, cx: &mut Context<Self>) {
        self.graph_name = name;
        cx.notify();
    }

    /// Replaces the favorites (`:favorites` of the config, in file order).
    pub fn set_favorites(&mut self, favorites: Vec<String>, cx: &mut Context<Self>) {
        self.favorites = favorites;
        cx.notify();
    }

    /// Replaces the recent pages (newest first).
    pub fn set_recent(&mut self, recent: Vec<String>, cx: &mut Context<Self>) {
        self.recent = recent;
        cx.notify();
    }

    /// The favorites shown.
    pub fn favorites(&self) -> &[String] {
        &self.favorites
    }

    /// The recent pages shown.
    pub fn recent(&self) -> &[String] {
        &self.recent
    }

    /// Highlights the entry of what the main area shows.
    pub fn set_current(&mut self, route: Option<&Route>, cx: &mut Context<Self>) {
        let active = match route {
            Some(Route::Journals) => Some(Target::Journals),
            Some(Route::AllPages) => Some(Target::AllPages),
            Some(Route::Page(name)) => Some(Target::Page(name.clone())),
            Some(Route::Block(_)) | None => None,
        };
        if self.active != active {
            self.active = active;
            cx.notify();
        }
    }

    /// Whether the sidebar is shown.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Shows or hides the sidebar.
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.visible != visible {
            self.visible = visible;
            cx.notify();
        }
    }

    /// Flips visibility (the `ToggleLeftSidebar` action).
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.set_visible(!self.visible, cx);
    }

    /// The last selected destination.
    pub fn active(&self) -> Option<&Target> {
        self.active.as_ref()
    }

    /// Selects a destination and emits [`SidebarEvent::Navigate`].
    pub fn select(&mut self, target: Target, cx: &mut Context<Self>) {
        self.active = Some(target.clone());
        cx.emit(SidebarEvent::Navigate(target));
        cx.notify();
    }

    fn is_active(&self, target: &Target) -> bool {
        match (&self.active, target) {
            (Some(Target::Page(a)), Target::Page(b)) => a.eq_ignore_ascii_case(b),
            (Some(a), b) => a == b,
            (None, _) => false,
        }
    }
}

impl Render for LeftSidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let item = |label: String, icon: IconName, target: Target, cx: &mut Context<Self>| {
            let active = self.is_active(&target);
            SidebarMenuItem::new(label)
                .icon(icon)
                .active(active)
                .on_click(cx.listener(move |this, _, window, cx| {
                    match &target {
                        Target::Page(name) if window.modifiers().shift => {
                            cx.emit(SidebarEvent::OpenInSidebar(name.clone()));
                        }
                        _ => this.select(target.clone(), cx),
                    };
                }))
        };
        let navigate = SidebarMenu::new()
            .child(item(
                t!("sidebar.journals").to_string(),
                IconName::Calendar,
                Target::Journals,
                cx,
            ))
            .child(item(
                t!("sidebar.all_pages").to_string(),
                IconName::FileText,
                Target::AllPages,
                cx,
            ));
        let page_list = |names: &[String], icon: IconName, cx: &mut Context<Self>| {
            let mut menu = SidebarMenu::new();
            if names.is_empty() {
                menu =
                    menu.child(SidebarMenuItem::new(t!("sidebar.empty").to_string()).disable(true));
            }
            for name in names {
                menu = menu.child(item(
                    name.clone(),
                    icon.clone(),
                    Target::Page(name.clone()),
                    cx,
                ));
            }
            menu
        };
        let favorites = self.favorites.clone();
        let recent = self.recent.clone();
        let favorites_menu = page_list(&favorites, IconName::Star, cx);
        let recent_menu = page_list(&recent, IconName::BookOpen, cx);
        let graph_title = self
            .graph_name
            .clone()
            .unwrap_or_else(|| t!("sidebar.no_graph").to_string());
        let switcher = SidebarMenu::new().child(item(
            t!("sidebar.graph_switcher").to_string(),
            IconName::Folder,
            Target::GraphSwitcher,
            cx,
        ));
        Sidebar::<SidebarGroup<SidebarMenu>>::new("left-sidebar")
            .collapsible(SidebarCollapsible::None)
            .header(SidebarHeader::new().child(graph_title))
            .child(SidebarGroup::new(t!("sidebar.group_navigate").to_string()).child(navigate))
            .child(
                SidebarGroup::new(t!("sidebar.group_favorites").to_string()).child(favorites_menu),
            )
            .child(SidebarGroup::new(t!("sidebar.group_recent").to_string()).child(recent_menu))
            .child(SidebarGroup::new(t!("sidebar.group_graph").to_string()).child(switcher))
            .h_full()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing::{TestAppContext, gpui_test};
    use crate::{settings::AppSettings, theme};

    fn setup(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            theme::install(cx, AppSettings::default(), None);
        });
    }

    #[gpui_test]
    fn current_route_highlights_its_entry(cx: &mut TestAppContext) {
        setup(cx);
        let (sidebar, cx) = cx.add_window_view(|_, _| LeftSidebar::new(Some("g".into())));
        sidebar.update(cx, |s, cx| {
            s.set_favorites(vec!["Alpha".into()], cx);
            s.set_recent(vec!["Beta".into(), "Alpha".into()], cx);
            s.set_current(Some(&Route::Page("alpha".into())), cx);
        });
        assert!(sidebar.read_with(cx, |s, _| s.is_active(&Target::Page("Alpha".into()))));
        assert!(!sidebar.read_with(cx, |s, _| s.is_active(&Target::Page("Beta".into()))));
        sidebar.update(cx, |s, cx| s.set_current(Some(&Route::AllPages), cx));
        assert_eq!(
            sidebar.read_with(cx, |s, _| s.active().cloned()),
            Some(Target::AllPages)
        );
        sidebar.update(cx, |s, cx| {
            s.set_current(Some(&Route::Block("x".into())), cx)
        });
        assert_eq!(sidebar.read_with(cx, |s, _| s.active().cloned()), None);
    }

    #[gpui_test]
    fn selecting_a_page_emits_navigate(cx: &mut TestAppContext) {
        setup(cx);
        let (sidebar, cx) = cx.add_window_view(|_, _| LeftSidebar::new(None));
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = seen.clone();
        let _sub = cx.update(|_, cx| {
            cx.subscribe(&sidebar, move |_, e: &SidebarEvent, _| {
                sink.borrow_mut().push(e.clone());
            })
        });
        sidebar.update(cx, |s, cx| s.select(Target::Page("Alpha".into()), cx));
        assert_eq!(
            *seen.borrow(),
            vec![SidebarEvent::Navigate(Target::Page("Alpha".into()))]
        );
    }
}
