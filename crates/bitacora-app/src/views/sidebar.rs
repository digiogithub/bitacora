//! Left sidebar (BIT-US-0079, redesigned by BIT-US-0123): 252px `side` column with the nav
//! (Journals, All pages, Tasks with the overdue count, Graph), a month calendar with a dot on
//! the days that have notes, Favorites (`:favorites` of `config.edn`, read-only here), Recent
//! pages and a footer with the graph, sync and MCP status. The entry of the page on screen is
//! highlighted.
//!
//! The index is read in the background ([`data::sidebar_data`]) whenever the graph or the
//! displayed month changes and, debounced, after index events.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use bitacora_core::date::Date;
use bitacora_mcp::TokenStore;
use rust_i18n::t;

use crate::data::{self, GraphHandle, SidebarData};
use crate::nav::{OpenIn, Route};
use crate::ui::text_edit::{ClipboardItem, FontWeight};
use crate::ui::theme::{ActiveBitacoraTheme as _, BitacoraTheme, TypeStyleExt as _};
use crate::ui::{
    Anchor, AnyElement, App, ClickEvent, Context, ElementId, EventEmitter, FluentBuilder as _,
    Hsla, InteractiveElement as _, IntoElement, Level, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Task, Window, anchored, deferred, div, h_flex,
    px, v_flex,
};
use crate::views::calendar::{
    CalendarHandlers, CalendarState, Month, month_bounds, render_calendar, shift_month,
};
use crate::views::dims;
use crate::views::kit::{Button, Glyph, Overline, PopoverShell, glyph};
use crate::views::mcp_connect::{McpCopy, McpPopover, copy_text, popover_state};
use crate::views::pando_status::PandoState;
use crate::views::status_bar::SlotState;

/// Pause after an index event before the sidebar reads the index again.
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(400);
/// Top padding of the column (`layout.md`: 14 / 12 / 12; 14 has no token).
const PAD_TOP: f32 = 14.0;
/// Height of a favorite / recent row (`layout.md`: 30-32).
const LIST_ROW: f32 = 30.0;
/// Diameter of a status dot in the footer.
const STATUS_DOT: f32 = 7.0;

/// Where a sidebar click wants to go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Journals index.
    Journals,
    /// All pages list.
    AllPages,
    /// The Tasks view (BIT-US-0126).
    Tasks,
    /// The graph view (BIT-US-0157).
    Graph,
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
    /// The user Ctrl/Cmd+clicked a page: open it in a new tab.
    OpenInNewTab(String),
    /// The user clicked a calendar day (`yyyyMMdd`): open that journal (nothing is created
    /// until it is edited).
    OpenJournalDay(u32),
    /// The user asked to jump to a date (the "Go to date..." link).
    GoToDate,
    /// The user clicked the Pando status row of the footer.
    OpenPando,
    /// The user asked for Settings > Agents from the MCP connection popover (BIT-US-0186).
    OpenMcpSettings,
}

/// Source of "today" (replaced in tests).
pub type Clock = Rc<dyn Fn() -> Option<Date>>;

impl std::fmt::Debug for LeftSidebar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LeftSidebar")
            .field("graph_name", &self.graph_name)
            .field("visible", &self.visible)
            .field("active", &self.active)
            .field("month", &self.month)
            .field("selected_day", &self.selected_day)
            .finish_non_exhaustive()
    }
}

/// The left sidebar view.
pub struct LeftSidebar {
    graph_name: Option<String>,
    visible: bool,
    active: Option<Target>,
    favorites: Vec<String>,
    recent: Vec<String>,
    clock: Clock,
    handle: Option<GraphHandle>,
    /// Displayed month; `None` follows today.
    month: Option<Month>,
    /// The journal day on screen (`yyyyMMdd`).
    selected_day: Option<u32>,
    data: SidebarData,
    generation: u64,
    load_task: Option<Task<()>>,
    refresh_task: Option<Task<()>>,
    mcp: SlotState,
    mcp_endpoint: Option<String>,
    /// Tokens of the running MCP server, for the connection popover (BIT-US-0186).
    mcp_tokens: Option<Arc<TokenStore>>,
    mcp_popover_open: bool,
    sync: SlotState,
    pando: PandoState,
}

impl EventEmitter<SidebarEvent> for LeftSidebar {}

impl LeftSidebar {
    /// Creates a visible sidebar for the (optional) open graph.
    pub fn new(graph_name: Option<String>) -> Self {
        Self::with_clock(graph_name, Rc::new(data::today_local))
    }

    /// Like [`new`](Self::new) with an injected clock.
    pub fn with_clock(graph_name: Option<String>, clock: Clock) -> Self {
        Self {
            graph_name,
            visible: true,
            active: None,
            favorites: Vec::new(),
            recent: Vec::new(),
            clock,
            handle: None,
            month: None,
            selected_day: None,
            data: SidebarData::default(),
            generation: 0,
            load_task: None,
            refresh_task: None,
            mcp: SlotState::Off,
            mcp_endpoint: None,
            mcp_tokens: None,
            mcp_popover_open: false,
            sync: SlotState::Off,
            pando: PandoState::NotConfigured,
        }
    }

    /// Sets the graph name shown in the footer.
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
            Some(Route::Graph) => Some(Target::Graph),
            Some(Route::Tasks) => Some(Target::Tasks),
            Some(Route::Page(name)) => Some(Target::Page(name.clone())),
            Some(Route::Block(_)) | None => None,
        };
        if self.active != active {
            self.active = active;
            cx.notify();
        }
    }

    /// Highlights a nav entry directly (for views that have no [`Route`], like Tasks / Graph).
    pub fn set_active_target(&mut self, target: Option<Target>, cx: &mut Context<Self>) {
        if self.active != target {
            self.active = target;
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

    /// A click on a favorite or recent page: plain navigates, Shift opens the right sidebar,
    /// Ctrl/Cmd opens a new tab.
    pub fn click_page(&mut self, page: &str, open: OpenIn, cx: &mut Context<Self>) {
        match open {
            OpenIn::Main => self.select(Target::Page(page.to_owned()), cx),
            OpenIn::Sidebar => cx.emit(SidebarEvent::OpenInSidebar(page.to_owned())),
            OpenIn::NewTab => cx.emit(SidebarEvent::OpenInNewTab(page.to_owned())),
        }
    }

    /// Connects the sidebar to the graph index (`None` on close) and reads it.
    pub fn set_handle(&mut self, handle: Option<GraphHandle>, cx: &mut Context<Self>) {
        self.handle = handle;
        self.data = SidebarData::default();
        self.load_task = None;
        self.refresh_task = None;
        self.reload(cx);
        cx.notify();
    }

    /// Any index change may touch a journal or a task: read again (debounced).
    pub fn on_index_changed(&mut self, cx: &mut Context<Self>) {
        if self.handle.is_none() {
            return;
        }
        self.refresh_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REFRESH_DEBOUNCE).await;
            let _ = this.update(cx, |sidebar, cx| sidebar.reload(cx));
        }));
    }

    /// The footer status: MCP slot, the endpoint it listens on and the sync slot.
    pub fn set_footer_status(
        &mut self,
        mcp: SlotState,
        mcp_endpoint: Option<String>,
        sync: SlotState,
        cx: &mut Context<Self>,
    ) {
        if (self.mcp, &self.mcp_endpoint, self.sync) != (mcp, &mcp_endpoint, sync) {
            self.mcp = mcp;
            self.mcp_endpoint = mcp_endpoint;
            self.sync = sync;
            cx.notify();
        }
    }

    /// The token store of the running MCP server (what the connection popover copies from).
    pub fn set_mcp_tokens(&mut self, tokens: Option<Arc<TokenStore>>, cx: &mut Context<Self>) {
        self.mcp_tokens = tokens;
        cx.notify();
    }

    /// Whether the MCP connection popover is open.
    pub fn mcp_popover_open(&self) -> bool {
        self.mcp_popover_open
    }

    /// Opens or closes the MCP connection popover.
    pub fn set_mcp_popover(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.mcp_popover_open != open {
            self.mcp_popover_open = open;
            cx.notify();
        }
    }

    /// Puts the text of `kind` on the clipboard, toasts the result and closes the popover.
    /// Secrets only travel to the clipboard; nothing here is logged.
    fn copy_mcp(
        &mut self,
        kind: McpCopy,
        token: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self
            .mcp_endpoint
            .as_deref()
            .and_then(|endpoint| copy_text(kind, endpoint, token, self.mcp_tokens.as_deref()));
        match text {
            Some(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                crate::ui::notify(
                    window,
                    cx,
                    Level::Success,
                    t!("sidebar.mcp_copied").to_string(),
                );
            }
            None => crate::ui::notify(
                window,
                cx,
                Level::Warning,
                t!("sidebar.mcp_secret_lost").to_string(),
            ),
        }
        self.set_mcp_popover(false, cx);
    }

    /// The popover anchored above the MCP row.
    fn mcp_popover(&self, theme: &BitacoraTheme, cx: &mut Context<Self>) -> AnyElement {
        let c = &theme.colors;
        let m = &theme.metrics;
        let tokens = self
            .mcp_tokens
            .as_ref()
            .map(|t| t.summaries())
            .unwrap_or_default();
        let state = popover_state(self.mcp, self.mcp_endpoint.as_deref(), &tokens);
        let settings_link = |id: &'static str, label: String, cx: &mut Context<Self>| {
            Button::new(id)
                .ghost()
                .label(label)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_mcp_popover(false, cx);
                    cx.emit(SidebarEvent::OpenMcpSettings);
                }))
        };
        let note = |id: &'static str, text: String| {
            div()
                .id(id)
                .debug_selector(move || id.to_string())
                .text_color(c.text_2)
                .type_style(&theme.type_scale.ui_small)
                .child(text)
        };
        let copy_button = |id: &'static str,
                           label: String,
                           kind: McpCopy,
                           token: String,
                           cx: &mut Context<Self>| {
            Button::new(id)
                .secondary()
                .label(label)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.copy_mcp(kind, &token, window, cx);
                }))
        };
        let mut body = v_flex().gap(m.space[4]).child(
            div()
                .type_style(&theme.type_scale.ui)
                .child(t!("sidebar.mcp_title").to_string()),
        );
        if let McpPopover::Ready { endpoint, .. } | McpPopover::NoToken { endpoint } = &state {
            body = body.child(
                div()
                    .id("mcp-popover-endpoint")
                    .debug_selector(|| "mcp-popover-endpoint".to_string())
                    .px(m.space[4])
                    .py(m.space[3])
                    .rounded(m.radius_control)
                    .bg(c.hover)
                    .text_color(c.text)
                    .type_style(&theme.type_scale.mono)
                    .child(endpoint.clone()),
            );
        }
        match &state {
            McpPopover::Ready { token, .. } => {
                body = body.child(
                    v_flex()
                        .gap(m.space[3])
                        .child(copy_button(
                            "mcp-copy-url",
                            t!("sidebar.mcp_copy_url").to_string(),
                            McpCopy::Url,
                            token.clone(),
                            cx,
                        ))
                        .child(copy_button(
                            "mcp-copy-json",
                            t!("sidebar.mcp_copy_json").to_string(),
                            McpCopy::Json,
                            token.clone(),
                            cx,
                        ))
                        .child(copy_button(
                            "mcp-copy-command",
                            t!("sidebar.mcp_copy_command").to_string(),
                            McpCopy::Command,
                            token.clone(),
                            cx,
                        ))
                        .child(settings_link(
                            "mcp-manage-tokens",
                            t!("sidebar.mcp_manage_tokens").to_string(),
                            cx,
                        )),
                );
            }
            McpPopover::NoToken { .. } => {
                body = body
                    .child(note(
                        "mcp-popover-note",
                        t!("sidebar.mcp_no_token").to_string(),
                    ))
                    .child(
                        v_flex()
                            .gap(m.space[3])
                            .child(copy_button(
                                "mcp-copy-url",
                                t!("sidebar.mcp_copy_url").to_string(),
                                McpCopy::Url,
                                String::new(),
                                cx,
                            ))
                            .child(settings_link(
                                "mcp-create-token",
                                t!("sidebar.mcp_create_token").to_string(),
                                cx,
                            )),
                    );
            }
            McpPopover::Off | McpPopover::Failed => {
                let text = if state == McpPopover::Off {
                    t!("sidebar.mcp_off")
                } else {
                    t!("sidebar.mcp_failed")
                };
                body = body
                    .child(note("mcp-popover-note", text.to_string()))
                    .child(settings_link(
                        "mcp-open-settings",
                        t!("sidebar.mcp_open_settings").to_string(),
                        cx,
                    ));
            }
        }
        let weak = cx.entity().downgrade();
        deferred(
            anchored()
                .anchor(Anchor::BottomLeft)
                .snap_to_window()
                .child(
                    div().mb(m.space[2]).child(
                        PopoverShell::new("mcp-popover")
                            .width(dims::PX_320)
                            .on_dismiss(move |_, cx| {
                                let _ = weak.update(cx, |this, cx| this.set_mcp_popover(false, cx));
                            })
                            .child(body),
                    ),
                ),
        )
        .priority(10)
        .into_any_element()
    }

    /// The Pando connection state shown in the footer.
    pub fn set_pando_state(&mut self, state: PandoState, cx: &mut Context<Self>) {
        if self.pando != state {
            self.pando = state;
            cx.notify();
        }
    }

    /// The Pando state of the footer.
    pub fn pando_state(&self) -> PandoState {
        self.pando
    }

    /// The journal on screen (`yyyyMMdd`); the calendar shows its month.
    pub fn set_selected_day(&mut self, day: Option<u32>, cx: &mut Context<Self>) {
        if self.selected_day == day {
            return;
        }
        self.selected_day = day;
        if let Some(date) = day.and_then(Date::from_journal_day) {
            let month = Month::of(date);
            if self.month.is_some() && self.displayed_month() != Some(month) {
                self.month = Some(month);
                self.reload(cx);
            }
        }
        cx.notify();
    }

    /// The journal day highlighted in the calendar.
    pub fn selected_day(&self) -> Option<u32> {
        self.selected_day
    }

    /// The month the calendar shows: the chosen one, else today's.
    pub fn displayed_month(&self) -> Option<Month> {
        self.month.or_else(|| (self.clock)().map(Month::of))
    }

    /// Moves the calendar by `delta` months and reads that month's dots.
    pub fn shift_month(&mut self, delta: i32, cx: &mut Context<Self>) {
        if let Some(current) = self.displayed_month() {
            self.month = Some(shift_month(current, delta));
            self.reload(cx);
            cx.notify();
        }
    }

    /// A calendar day was clicked.
    pub fn click_day(&mut self, day: u32, cx: &mut Context<Self>) {
        self.selected_day = Some(day);
        cx.emit(SidebarEvent::OpenJournalDay(day));
        cx.notify();
    }

    /// Overdue tasks as of the last read.
    pub fn overdue(&self) -> usize {
        self.data.overdue
    }

    /// Days of the displayed month with notes, as of the last read.
    pub fn days_with_notes(&self) -> &std::collections::HashSet<u32> {
        &self.data.with_notes
    }

    /// Reads the index for the displayed month in the background.
    fn reload(&mut self, cx: &mut Context<Self>) {
        let (Some(handle), Some(month)) = (self.handle.clone(), self.displayed_month()) else {
            return;
        };
        let today = (self.clock)();
        let (first, last) = month_bounds(month);
        self.generation += 1;
        let generation = self.generation;
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { data::sidebar_data(&handle, first, last, today) })
                .await;
            let _ = this.update(cx, |sidebar, cx| {
                if sidebar.generation != generation {
                    return;
                }
                match result {
                    Ok(data) => {
                        sidebar.data = data;
                        cx.notify();
                    }
                    Err(message) => tracing::warn!("cannot read the sidebar data: {message}"),
                }
            });
        }));
    }

    fn is_active(&self, target: &Target) -> bool {
        match (&self.active, target) {
            (Some(Target::Page(a)), Target::Page(b)) => a.eq_ignore_ascii_case(b),
            (Some(a), b) => a == b,
            (None, _) => false,
        }
    }
}

/// Colours of a nav / list row: `(fill, text, weight)`. The active row has the `hover` fill,
/// `text` colour and weight 600; the rest is transparent with `text_2`.
#[must_use]
pub fn row_look(active: bool, theme: &BitacoraTheme) -> (Option<Hsla>, Hsla, FontWeight) {
    let c = &theme.colors;
    if active {
        (Some(c.hover), c.text, FontWeight::SEMIBOLD)
    } else {
        (None, c.text_2, FontWeight::NORMAL)
    }
}

/// `host:port` of an MCP endpoint URL, for the footer transport line.
#[must_use]
pub fn endpoint_label(endpoint: &str) -> String {
    let rest = endpoint.split_once("://").map_or(endpoint, |(_, r)| r);
    let host = rest.split('/').next().unwrap_or(rest);
    format!("HTTP {host}")
}

/// Footer dot colour of a slot: `ok` while running, `warn` on error, `muted` when off.
#[must_use]
pub fn status_dot(state: SlotState, theme: &BitacoraTheme) -> Hsla {
    match state {
        SlotState::Idle | SlotState::Busy => theme.colors.ok,
        SlotState::Error => theme.colors.warn,
        SlotState::Off => theme.colors.muted,
    }
}

impl LeftSidebar {
    #[allow(clippy::too_many_arguments)]
    fn nav_item(
        &self,
        id: &'static str,
        icon: Glyph,
        label: String,
        target: Target,
        trailing: Option<AnyElement>,
        theme: &BitacoraTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let m = &theme.metrics;
        let (fill, fg, weight) = row_look(self.is_active(&target), theme);
        let hover = theme.colors.hover;
        h_flex()
            .id(ElementId::from(id))
            .debug_selector(|| id.to_string())
            .h(m.nav_item)
            .px(m.space[5])
            .gap(m.space[5])
            .items_center()
            .rounded(m.radius_control)
            .cursor_pointer()
            .text_color(fg)
            .type_style(&theme.type_scale.ui)
            .font_weight(weight)
            .when_some(fill, |d, fill| d.bg(fill))
            .when(fill.is_none(), |d| d.hover(move |s| s.bg(hover)))
            .on_click(cx.listener(move |this, _, _, cx| this.select(target.clone(), cx)))
            .child(glyph(icon, m.icon, fg, cx))
            .child(div().flex_1().min_w_0().truncate().child(label))
            .when_some(trailing, |d, t| d.child(t))
            .into_any_element()
    }

    fn page_list(
        &self,
        id: &'static str,
        names: &[String],
        theme: &BitacoraTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let m = &theme.metrics;
        let mut list = v_flex();
        if names.is_empty() {
            return list
                .child(
                    div()
                        .h(px(LIST_ROW))
                        .px(m.space[5])
                        .flex()
                        .items_center()
                        .text_color(theme.colors.muted)
                        .type_style(&theme.type_scale.caption)
                        .child(t!("sidebar.empty").to_string()),
                )
                .into_any_element();
        }
        for (ix, name) in names.iter().enumerate() {
            let target = Target::Page(name.clone());
            let (fill, fg, weight) = row_look(self.is_active(&target), theme);
            let hover = theme.colors.hover;
            let page = name.clone();
            list = list.child(
                div()
                    .id(ElementId::from((id, ix)))
                    .h(px(LIST_ROW))
                    .px(m.space[5])
                    .flex()
                    .items_center()
                    .rounded(m.radius_control)
                    .cursor_pointer()
                    .text_color(fg)
                    .type_style(&theme.type_scale.ui)
                    .font_weight(weight)
                    .when_some(fill, |d, fill| d.bg(fill))
                    .when(fill.is_none(), |d| d.hover(move |s| s.bg(hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.click_page(&page, OpenIn::from_modifiers(&window.modifiers()), cx);
                    }))
                    .child(div().min_w_0().truncate().child(name.clone())),
            );
        }
        list.into_any_element()
    }

    fn footer(&self, theme: &BitacoraTheme, cx: &mut Context<Self>) -> AnyElement {
        let m = &theme.metrics;
        let c = &theme.colors;
        let dot = |colour: Hsla| {
            div()
                .flex_shrink_0()
                .size(px(STATUS_DOT))
                .rounded_full()
                .bg(colour)
        };
        let graph_title = self
            .graph_name
            .clone()
            .unwrap_or_else(|| t!("sidebar.no_graph").to_string());
        let pages = self.handle.is_some().then_some(self.data.pages);
        let graph_row = h_flex()
            .id("sidebar-graph")
            .debug_selector(|| "sidebar-graph".to_string())
            .h(px(LIST_ROW))
            .px(m.space[5])
            .gap(m.space[4])
            .items_center()
            .rounded(m.radius_control)
            .cursor_pointer()
            .hover({
                let hover = c.hover;
                move |s| s.bg(hover)
            })
            .text_color(c.text)
            .type_style(&theme.type_scale.ui_small)
            .on_click(cx.listener(|this, _, _, cx| this.select(Target::GraphSwitcher, cx)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(format!("{} \u{b7} {graph_title}", t!("sidebar.graph"))),
            )
            .when_some(pages, |d, n| {
                d.child(
                    div()
                        .flex_shrink_0()
                        .text_color(c.muted)
                        .type_style(&theme.type_scale.mono)
                        .child(n.to_string()),
                )
            });
        let mcp_label = match self.mcp {
            SlotState::Idle | SlotState::Busy => t!("sidebar.mcp_active").to_string(),
            SlotState::Off => t!("status.mcp.off").to_string(),
            SlotState::Error => t!("status.mcp.error").to_string(),
        };
        let transport = match (self.mcp, &self.mcp_endpoint) {
            (SlotState::Idle | SlotState::Busy, Some(endpoint)) => Some(endpoint_label(endpoint)),
            _ => None,
        };
        let mcp_open = self.mcp_popover_open;
        let mcp_row = h_flex()
            .id("sidebar-mcp")
            .debug_selector(|| "sidebar-mcp".to_string())
            .px(m.space[5])
            .gap(m.space[4])
            .items_center()
            .rounded(m.radius_control)
            .cursor_pointer()
            .when(mcp_open, |d| d.bg(c.hover))
            .hover({
                let hover = c.hover;
                move |s| s.bg(hover)
            })
            .tooltip(|window, cx| {
                crate::ui::Tooltip::new(t!("sidebar.mcp_tooltip").to_string()).build(window, cx)
            })
            .text_color(c.text_2)
            .type_style(&theme.type_scale.caption)
            .on_click(cx.listener(|this, _, _, cx| {
                let open = !this.mcp_popover_open;
                this.set_mcp_popover(open, cx);
            }))
            .child(dot(status_dot(self.mcp, theme)))
            .child(div().min_w_0().truncate().child(mcp_label))
            .when_some(transport, |d, text| {
                d.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(c.muted)
                        .type_style(&theme.type_scale.mono)
                        .child(text),
                )
            });
        let mcp_popover = self.mcp_popover_open.then(|| self.mcp_popover(theme, cx));
        let mcp_row = div()
            .relative()
            .when_some(mcp_popover, |d, p| d.child(p))
            .child(mcp_row);
        let sync_label = match self.sync {
            SlotState::Off => t!("status.sync.off"),
            SlotState::Idle => t!("status.sync.idle"),
            SlotState::Busy => t!("status.sync.busy"),
            SlotState::Error => t!("status.sync.error"),
        }
        .to_string();
        let sync_row = h_flex()
            .id("sidebar-sync")
            .px(m.space[5])
            .gap(m.space[4])
            .items_center()
            .text_color(c.text_2)
            .type_style(&theme.type_scale.caption)
            .child(dot(status_dot(self.sync, theme)))
            .child(div().min_w_0().truncate().child(sync_label));
        let pando_row = h_flex()
            .id("sidebar-pando")
            .debug_selector(|| "sidebar-pando".to_string())
            .px(m.space[5])
            .gap(m.space[4])
            .items_center()
            .cursor_pointer()
            .text_color(c.text_2)
            .type_style(&theme.type_scale.caption)
            .on_click(cx.listener(|_, _, _, cx| cx.emit(SidebarEvent::OpenPando)))
            .child(dot(self.pando.dot(theme)))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(t!("pando_status.footer", state = self.pando.label()).to_string()),
            );
        v_flex()
            .flex_shrink_0()
            .gap(m.space[2])
            .pt(m.space[5])
            .border_t_1()
            .border_color(c.line)
            .child(graph_row)
            .child(mcp_row)
            .child(sync_row)
            .child(pando_row)
            .into_any_element()
    }
}

impl Render for LeftSidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.bitacora().clone();
        let m = &theme.metrics;
        let c = &theme.colors;

        let overdue = (self.data.overdue > 0).then(|| {
            div()
                .id("sidebar-overdue")
                .debug_selector(|| "sidebar-overdue".to_string())
                .flex_shrink_0()
                .text_color(c.warn)
                .type_style(&theme.type_scale.mono)
                .child(self.data.overdue.to_string())
                .into_any_element()
        });
        let nav = v_flex()
            .gap(m.space[1])
            .child(self.nav_item(
                "sidebar-journals",
                Glyph::Calendar,
                t!("sidebar.journals").to_string(),
                Target::Journals,
                None,
                &theme,
                cx,
            ))
            .child(self.nav_item(
                "sidebar-all-pages",
                Glyph::File,
                t!("sidebar.all_pages").to_string(),
                Target::AllPages,
                None,
                &theme,
                cx,
            ))
            .child(self.nav_item(
                "sidebar-tasks",
                Glyph::SquareCheck,
                t!("sidebar.tasks").to_string(),
                Target::Tasks,
                overdue,
                &theme,
                cx,
            ))
            .child(self.nav_item(
                "sidebar-graph-view",
                Glyph::Network,
                t!("sidebar.graph").to_string(),
                Target::Graph,
                None,
                &theme,
                cx,
            ));

        let calendar = self.displayed_month().map(|month| {
            let state = CalendarState {
                month,
                today: (self.clock)(),
                selected: self.selected_day,
                with_notes: self.data.with_notes.clone(),
            };
            let weak = cx.entity().downgrade();
            let on_day = {
                let weak = weak.clone();
                Rc::new(move |day: u32, _: &mut Window, cx: &mut App| {
                    let _ = weak.update(cx, |s, cx| s.click_day(day, cx));
                })
            };
            let on_shift = Rc::new(move |delta: i32, _: &mut Window, cx: &mut App| {
                let _ = weak.update(cx, |s, cx| s.shift_month(delta, cx));
            });
            let handlers = CalendarHandlers { on_day, on_shift };
            v_flex()
                .gap(m.space[3])
                .child(render_calendar(&state, &handlers, cx))
                .child(
                    div()
                        .id("sidebar-go-to-date")
                        .px(m.space[5])
                        .cursor_pointer()
                        .text_color(c.accent)
                        .type_style(&theme.type_scale.caption)
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(SidebarEvent::GoToDate)))
                        .child(t!("sidebar.go_to_date").to_string()),
                )
        });

        let favorites = self.favorites.clone();
        let recent = self.recent.clone();
        let favorites_list = self.page_list("sidebar-favorite", &favorites, &theme, cx);
        let recent_list = self.page_list("sidebar-recent", &recent, &theme, cx);
        let group = |overline: Overline, list: AnyElement| {
            v_flex()
                .gap(m.space[2])
                .child(div().px(m.space[5]).child(overline))
                .child(list)
        };
        let footer = self.footer(&theme, cx);

        v_flex()
            .id("left-sidebar")
            .debug_selector(|| "left-sidebar".to_string())
            .w(m.sidebar_left)
            .flex_shrink_0()
            .h_full()
            .pt(px(PAD_TOP))
            .px(m.space[6])
            .pb(m.space[6])
            .gap(m.space[8])
            .bg(c.side)
            .border_r_1()
            .border_color(c.line)
            .child(
                v_flex()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h_0()
                    .gap(m.space[8])
                    .overflow_y_scroll()
                    .child(nav)
                    .children(calendar)
                    .child(group(
                        Overline::new(t!("sidebar.group_favorites").to_string())
                            .count(favorites.len()),
                        favorites_list,
                    ))
                    .child(group(
                        Overline::new(t!("sidebar.group_recent").to_string()).count(recent.len()),
                        recent_list,
                    )),
            )
            .child(footer)
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

    fn fixed_clock() -> Clock {
        Rc::new(|| Date::new(2026, 10, 7))
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
        for (route, target) in [(Route::Tasks, Target::Tasks), (Route::Graph, Target::Graph)] {
            sidebar.update(cx, |s, cx| s.set_current(Some(&route), cx));
            assert_eq!(
                sidebar.read_with(cx, |s, _| s.active().cloned()),
                Some(target)
            );
        }
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

    #[gpui_test]
    fn page_clicks_route_by_modifier(cx: &mut TestAppContext) {
        setup(cx);
        let (sidebar, cx) = cx.add_window_view(|_, _| LeftSidebar::new(None));
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = seen.clone();
        let _sub = cx.update(|_, cx| {
            cx.subscribe(&sidebar, move |_, e: &SidebarEvent, _| {
                sink.borrow_mut().push(e.clone());
            })
        });
        sidebar.update(cx, |s, cx| {
            s.click_page("Alpha", OpenIn::Main, cx);
            s.click_page("Alpha", OpenIn::Sidebar, cx);
            s.click_page("Alpha", OpenIn::NewTab, cx);
        });
        assert_eq!(
            *seen.borrow(),
            vec![
                SidebarEvent::Navigate(Target::Page("Alpha".into())),
                SidebarEvent::OpenInSidebar("Alpha".into()),
                SidebarEvent::OpenInNewTab("Alpha".into()),
            ]
        );
    }

    #[gpui_test]
    fn rendered_sidebar_has_the_design_width_and_nav_entries(cx: &mut TestAppContext) {
        setup(cx);
        let (sidebar, cx) =
            cx.add_window_view(|_, _| LeftSidebar::with_clock(Some("digio".into()), fixed_clock()));
        sidebar.update(cx, |s, cx| {
            s.data.overdue = 3;
            s.set_footer_status(
                SlotState::Idle,
                Some("http://127.0.0.1:4711/mcp".into()),
                SlotState::Idle,
                cx,
            );
        });
        cx.run_until_parked();
        let width = cx.update(|_, cx| cx.bitacora().metrics.sidebar_left);
        let bounds = cx.debug_bounds("left-sidebar").expect("sidebar painted");
        assert_eq!(bounds.size.width, width);
        for id in [
            "sidebar-journals",
            "sidebar-all-pages",
            "sidebar-tasks",
            "sidebar-graph-view",
            "sidebar-overdue",
            "calendar-day-20261007",
            "sidebar-mcp",
        ] {
            assert!(cx.debug_bounds(id).is_some(), "{id} is painted");
        }
    }

    #[gpui_test]
    fn nav_and_calendar_clicks_emit_events(cx: &mut TestAppContext) {
        setup(cx);
        let (sidebar, cx) = cx.add_window_view(|_, _| LeftSidebar::with_clock(None, fixed_clock()));
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = seen.clone();
        let _sub = cx.update(|_, cx| {
            cx.subscribe(&sidebar, move |_, e: &SidebarEvent, _| {
                sink.borrow_mut().push(e.clone());
            })
        });
        cx.run_until_parked();
        for id in [
            "sidebar-tasks",
            "sidebar-graph-view",
            "calendar-day-20261003",
        ] {
            let bounds = cx.debug_bounds(id).expect("painted");
            cx.simulate_click(bounds.center(), Default::default());
        }
        assert_eq!(
            *seen.borrow(),
            vec![
                SidebarEvent::Navigate(Target::Tasks),
                SidebarEvent::Navigate(Target::Graph),
                SidebarEvent::OpenJournalDay(20_261_003),
            ]
        );
        assert_eq!(
            sidebar.read_with(cx, |s, _| s.selected_day()),
            Some(20_261_003)
        );
    }

    #[gpui_test]
    fn clicking_the_mcp_row_opens_the_connection_popover(cx: &mut TestAppContext) {
        setup(cx);
        let (sidebar, cx) = cx.add_window_view(|_, _| LeftSidebar::with_clock(None, fixed_clock()));
        // Server off: the popover says so and links to the settings.
        cx.run_until_parked();
        let row = cx.debug_bounds("sidebar-mcp").expect("row painted");
        cx.simulate_click(row.center(), Default::default());
        assert!(sidebar.read_with(cx, |s, _| s.mcp_popover_open()));
        assert!(cx.debug_bounds("mcp-popover-note").is_some());
        assert!(cx.debug_bounds("mcp-popover-endpoint").is_none());
        sidebar.update(cx, |s, cx| s.set_mcp_popover(false, cx));
        // Running without a token: the URL is shown, with the hint to create a token.
        sidebar.update(cx, |s, cx| {
            s.set_footer_status(
                SlotState::Idle,
                Some("http://127.0.0.1:4711/mcp".into()),
                SlotState::Idle,
                cx,
            );
            s.set_mcp_popover(true, cx);
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("mcp-popover-endpoint").is_some());
        assert!(cx.debug_bounds("mcp-popover-note").is_some());
    }

    #[gpui_test]
    fn month_arrows_move_the_calendar(cx: &mut TestAppContext) {
        setup(cx);
        let (sidebar, cx) = cx.add_window_view(|_, _| LeftSidebar::with_clock(None, fixed_clock()));
        let shown = sidebar.read_with(cx, |s, _| s.displayed_month());
        assert_eq!(
            shown,
            Some(Month {
                year: 2026,
                month: 10
            })
        );
        sidebar.update(cx, |s, cx| s.shift_month(-1, cx));
        sidebar.update(cx, |s, cx| s.shift_month(-1, cx));
        let shown = sidebar.read_with(cx, |s, _| s.displayed_month());
        assert_eq!(
            shown,
            Some(Month {
                year: 2026,
                month: 8
            })
        );
        sidebar.update(cx, |s, cx| s.shift_month(5, cx));
        let shown = sidebar.read_with(cx, |s, _| s.displayed_month());
        assert_eq!(
            shown,
            Some(Month {
                year: 2027,
                month: 1
            })
        );
    }

    #[test]
    fn row_look_and_footer_helpers() {
        use crate::ui::theme::Mode;
        let theme = BitacoraTheme::new(Mode::Light);
        let c = theme.colors;
        assert_eq!(
            row_look(true, &theme),
            (Some(c.hover), c.text, FontWeight::SEMIBOLD)
        );
        assert_eq!(
            row_look(false, &theme),
            (None, c.text_2, FontWeight::NORMAL)
        );
        assert_eq!(
            endpoint_label("http://127.0.0.1:4711/mcp"),
            "HTTP 127.0.0.1:4711"
        );
        assert_eq!(status_dot(SlotState::Idle, &theme), c.ok);
        assert_eq!(status_dot(SlotState::Error, &theme), c.warn);
        assert_eq!(status_dot(SlotState::Off, &theme), c.muted);
    }
}
