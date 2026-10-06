//! Placeholder dock panels: the center page host and the right sidebar.

use rust_i18n::t;

use crate::ui::dock::{BasePanel, Panel, PanelEvent, panel_handle, register_panel};
use crate::ui::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, Global,
    IntoElement, ParentElement as _, Render, SharedString, Styled as _, Window, div,
};
use crate::views::page_view::PageView;

/// Persisted panel name of the center page host. Never change: it is in layout files.
pub const PAGE_HOST: &str = "PageHost";
/// Persisted panel name of the right sidebar. Never change: it is in layout files.
pub const RIGHT_SIDEBAR: &str = "RightSidebar";

/// Which placeholder a [`PlaceholderPanel`] stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    /// Center area that will host the open page(s).
    PageHost,
    /// Right dock (the future Logseq shift-click target).
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

/// A panel that only shows a localized placeholder text.
#[derive(Debug)]
pub struct PlaceholderPanel {
    kind: PanelKind,
    focus: FocusHandle,
    page: Option<Entity<PageView>>,
}

/// The shared page view, set by the workspace before panels are built so that a restored
/// layout's `PageHost` panel shows the same view as a freshly built one.
#[derive(Debug)]
pub struct SharedPageView(pub Entity<PageView>);

impl Global for SharedPageView {}

impl PlaceholderPanel {
    /// Creates the panel entity.
    pub fn new(kind: PanelKind, cx: &mut Context<Self>) -> Self {
        let page = (kind == PanelKind::PageHost)
            .then(|| cx.try_global::<SharedPageView>().map(|g| g.0.clone()))
            .flatten();
        Self {
            kind,
            focus: cx.focus_handle(),
            page,
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
        if let Some(page) = &self.page {
            return div().size_full().child(page.clone()).into_any_element();
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
        register_panel(cx, kind.name(), move |_, _, cx| {
            panel_handle(cx.new(|cx| PlaceholderPanel::new(kind, cx)))
        });
    }
}
