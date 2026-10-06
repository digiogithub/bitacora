//! Left sidebar: Journals, All pages, Favorites, Recent and a graph switcher.

use rust_i18n::t;

use crate::ui::sidebar::{
    Sidebar, SidebarCollapsible, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem,
};
use crate::ui::{
    Context, EventEmitter, IconName, IntoElement, ParentElement as _, Render, Styled as _, Window,
};

/// Where a sidebar click wants to go. Targets are stubs until navigation exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Journals index.
    Journals,
    /// All pages list.
    AllPages,
    /// Favorites section.
    Favorites,
    /// Recently visited pages.
    Recent,
    /// Graph switcher.
    GraphSwitcher,
}

/// Events emitted by the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarEvent {
    /// The user picked a destination.
    Navigate(Target),
}

/// The left sidebar view.
#[derive(Debug)]
pub struct LeftSidebar {
    graph_name: Option<String>,
    visible: bool,
    active: Option<Target>,
}

impl EventEmitter<SidebarEvent> for LeftSidebar {}

impl LeftSidebar {
    /// Creates a visible sidebar for the (optional) open graph.
    pub fn new(graph_name: Option<String>) -> Self {
        Self {
            graph_name,
            visible: true,
            active: None,
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
    pub fn active(&self) -> Option<Target> {
        self.active
    }

    /// Selects a destination and emits [`SidebarEvent::Navigate`].
    pub fn select(&mut self, target: Target, cx: &mut Context<Self>) {
        self.active = Some(target);
        cx.emit(SidebarEvent::Navigate(target));
        cx.notify();
    }
}

impl Render for LeftSidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let item = |label: String, icon: IconName, target: Target, cx: &mut Context<Self>| {
            SidebarMenuItem::new(label)
                .icon(icon)
                .active(self.active == Some(target))
                .on_click(cx.listener(move |this, _, _, cx| this.select(target, cx)))
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
            ))
            .child(item(
                t!("sidebar.favorites").to_string(),
                IconName::Star,
                Target::Favorites,
                cx,
            ))
            .child(item(
                t!("sidebar.recent").to_string(),
                IconName::BookOpen,
                Target::Recent,
                cx,
            ));
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
                SidebarGroup::new(t!("sidebar.group_favorites").to_string())
                    .child(SidebarMenu::new().child(
                        SidebarMenuItem::new(t!("sidebar.empty").to_string()).disable(true),
                    )),
            )
            .child(
                SidebarGroup::new(t!("sidebar.group_recent").to_string())
                    .child(SidebarMenu::new().child(
                        SidebarMenuItem::new(t!("sidebar.empty").to_string()).disable(true),
                    )),
            )
            .child(SidebarGroup::new(t!("sidebar.group_graph").to_string()).child(switcher))
            .h_full()
    }
}
