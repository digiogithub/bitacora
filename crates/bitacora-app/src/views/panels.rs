//! Dock panels: the center page host and the right sidebar.
//!
//! Every `PageHost` panel owns its own [`MainView`] (a *pane*: its own history, journals feed,
//! all-pages table and page view), so the main area can be split without sharing state. Panels
//! are created either by the workspace's default layout or by the dock when it restores a
//! persisted layout; in both cases they obtain their content from the [`PaneHub`], the
//! workspace's registry of panes and of the right sidebar stack.

use rust_i18n::t;

use crate::data::GraphHandle;
use crate::nav::Route;
use crate::ui::dock::{BasePanel, Panel, PanelEvent, panel_handle, register_panel};
use crate::ui::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, Global,
    IntoElement, ParentElement as _, Render, SharedString, Styled as _, WeakEntity, Window, div,
};
use crate::views::main_view::MainView;
use crate::views::right_sidebar::RightSidebar;

/// Persisted panel name of the center page host. Never change: it is in layout files.
pub const PAGE_HOST: &str = "PageHost";
/// Persisted panel name of the right sidebar. Never change: it is in layout files.
pub const RIGHT_SIDEBAR: &str = "RightSidebar";

/// Which content a [`PlaceholderPanel`] hosts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    /// Center area hosting one pane (a [`MainView`]).
    PageHost,
    /// Right dock hosting the sidebar stack (the Shift+click target).
    RightSidebar,
}

impl PanelKind {
    fn name(self) -> &'static str {
        match self {
            Self::PageHost => PAGE_HOST,
            Self::RightSidebar => RIGHT_SIDEBAR,
        }
    }
}

/// Hub events.
#[derive(Debug, Clone)]
pub enum HubEvent {
    /// A panel created a new pane; the workspace wires it up.
    PaneAdded(Entity<MainView>),
}

/// Registry of the panes and of the sidebar stack, shared with panels through
/// [`SharedHub`].
pub struct PaneHub {
    spare: Option<Entity<MainView>>,
    panes: Vec<WeakEntity<MainView>>,
    stack: Entity<RightSidebar>,
    handle: Option<GraphHandle>,
}

impl std::fmt::Debug for PaneHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaneHub")
            .field("panes", &self.panes.len())
            .finish_non_exhaustive()
    }
}

impl EventEmitter<HubEvent> for PaneHub {}

impl PaneHub {
    /// A hub whose first panel will get `primary`.
    pub fn new(primary: Entity<MainView>, stack: Entity<RightSidebar>) -> Self {
        Self {
            panes: vec![primary.downgrade()],
            spare: Some(primary),
            stack,
            handle: None,
        }
    }

    /// The live panes, primary first.
    pub fn panes(&self) -> Vec<Entity<MainView>> {
        self.panes.iter().filter_map(WeakEntity::upgrade).collect()
    }

    /// The right sidebar stack.
    pub fn stack(&self) -> &Entity<RightSidebar> {
        &self.stack
    }

    /// Remembers the open graph (panes created later connect to it).
    pub fn set_handle(&mut self, handle: Option<GraphHandle>) {
        self.handle = handle;
    }

    /// The pane for a new `PageHost` panel: the primary one first, a fresh one afterwards.
    pub fn claim_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<MainView> {
        if let Some(pane) = self.spare.take() {
            return pane;
        }
        let pane = cx.new(|cx| MainView::new(window, cx));
        self.panes.retain(|p| p.upgrade().is_some());
        self.panes.push(pane.downgrade());
        if let Some(handle) = self.handle.clone() {
            pane.update(cx, |p, cx| p.set_graph(handle, Some(Route::Journals), cx));
        }
        cx.emit(HubEvent::PaneAdded(pane.clone()));
        pane
    }
}

/// The workspace's hub, set before panels are built so that a restored layout's panels find it.
#[derive(Debug)]
pub struct SharedHub(pub Entity<PaneHub>);

impl Global for SharedHub {}

/// A dock panel hosting a pane or the sidebar stack.
#[derive(Debug)]
pub struct PlaceholderPanel {
    kind: PanelKind,
    focus: FocusHandle,
    pane: Option<Entity<MainView>>,
    stack: Option<Entity<RightSidebar>>,
}

impl PlaceholderPanel {
    /// Creates the panel entity; content comes from the shared hub when there is one.
    pub fn new(kind: PanelKind, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let hub = cx.try_global::<SharedHub>().map(|g| g.0.clone());
        let (pane, stack) = match (kind, hub) {
            (PanelKind::PageHost, Some(hub)) => (
                Some(hub.update(cx, |hub, cx| hub.claim_pane(window, cx))),
                None,
            ),
            (PanelKind::RightSidebar, Some(hub)) => (None, Some(hub.read(cx).stack().clone())),
            (_, None) => (None, None),
        };
        Self {
            kind,
            focus: cx.focus_handle(),
            pane,
            stack,
        }
    }
}

impl BasePanel for PlaceholderPanel {
    fn panel_name(&self) -> &'static str {
        self.kind.name()
    }

    fn closable(&self, _: &App) -> bool {
        false
    }
}

impl Panel for PlaceholderPanel {
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some(match self.kind {
            PanelKind::PageHost => t!("panel.page_host.title").to_string().into(),
            PanelKind::RightSidebar => t!("panel.right.title").to_string().into(),
        })
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        match self.kind {
            PanelKind::PageHost => t!("panel.page_host.title").to_string(),
            PanelKind::RightSidebar => t!("panel.right.title").to_string(),
        }
    }
}

impl EventEmitter<PanelEvent> for PlaceholderPanel {}

impl Focusable for PlaceholderPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PlaceholderPanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if let Some(pane) = &self.pane {
            return div().size_full().child(pane.clone()).into_any_element();
        }
        if let Some(stack) = &self.stack {
            return div().size_full().child(stack.clone()).into_any_element();
        }
        let text = match self.kind {
            PanelKind::PageHost => t!("panel.page_host.empty").to_string(),
            PanelKind::RightSidebar => t!("panel.right.empty").to_string(),
        };
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(text)
            .into_any_element()
    }
}

/// Registers both panels so a persisted layout can rebuild them by name.
pub fn register_panels(cx: &mut App) {
    for kind in [PanelKind::PageHost, PanelKind::RightSidebar] {
        register_panel(cx, kind.name(), move |_, window, cx| {
            panel_handle(cx.new(|cx| PlaceholderPanel::new(kind, window, cx)))
        });
    }
}
