//! Placeholder dock panels: the center page host and the right sidebar.

use rust_i18n::t;

use crate::ui::dock::{BasePanel, Panel, PanelEvent, panel_handle, register_panel};
use crate::ui::{
    App, AppContext as _, Context, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, div,
};

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
}

impl PlaceholderPanel {
    /// Creates the panel entity.
    pub fn new(kind: PanelKind, cx: &mut Context<Self>) -> Self {
        Self {
            kind,
            focus: cx.focus_handle(),
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
